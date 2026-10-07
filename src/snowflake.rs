// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 核心生成器 —— 对应 PHP 版的 `Snowflake` 类，全库唯一的有状态类型。

use std::fmt;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::parse::{ParsedId, parse_with_layout};
use crate::resolver::SequenceResolver;
use crate::resolvers::SequentialSequenceResolver;

/// 时钟回拨超出容忍值时的处理策略（对应配置项 `clock_drift_strategy`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockDriftStrategy {
    /// 直接拒绝生成（默认）。
    #[default]
    Throw,
    /// 自旋等待墙钟追平；超过 `clock_drift_wait_ms` 仍未追平则报错。
    /// 偶发 NTP 步进不该失败一个请求时用它。
    Wait,
}

impl ClockDriftStrategy {
    /// 配置里使用的字符串形式。
    pub fn as_str(self) -> &'static str {
        match self {
            ClockDriftStrategy::Throw => "throw",
            ClockDriftStrategy::Wait => "wait",
        }
    }
}

impl fmt::Display for ClockDriftStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ClockDriftStrategy {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "throw" => Ok(ClockDriftStrategy::Throw),
            "wait" => Ok(ClockDriftStrategy::Wait),
            other => Err(Error::InvalidConfig(format!(
                "Clock drift strategy must be \"throw\" or \"wait\", got \"{other}\"."
            ))),
        }
    }
}

/// Snowflake ID 生成器：64 位、时间有序、跨节点唯一。
///
/// `id()` 需要 `&mut self` —— 与 PHP 版「不要跨协程/线程共享实例」的警告
/// 不同，Rust 里这个错误**编译不过**；多线程共享请用 [`crate::Shared`]。
///
/// ```
/// use snowflake::Snowflake;
///
/// let mut snowflake = Snowflake::default();
/// let id = snowflake.id()?;
/// let parsed = snowflake.parse_id(id);
/// assert_eq!(parsed.worker_id, 0);
/// # Ok::<(), snowflake::Error>(())
/// ```
pub struct Snowflake {
    epoch: i64,
    worker_id: i64,
    datacenter_id: i64,
    sequence_bits: u32,
    worker_bits: u32,
    datacenter_bits: u32,
    max_sequence: i64,
    timestamp_shift: u32,
    max_timestamp_offset: i64,
    clock_tolerance_ms: i64,
    clock_drift_strategy: ClockDriftStrategy,
    clock_drift_wait_ms: i64,
    /// 预计算的 `(datacenter_id << datacenter_shift) | (worker_id << worker_shift)`。
    fixed_bits: i64,
    last_timestamp: i64,
    sequence_resolver: Box<dyn SequenceResolver>,
}

impl Snowflake {
    /// 默认 epoch：2024-01-01 00:00:00 UTC。
    pub const DEFAULT_EPOCH: i64 = 1_704_067_200_000;
    /// 默认 worker 位宽。
    pub const DEFAULT_WORKER_BITS: u32 = 5;
    /// 默认 datacenter 位宽。
    pub const DEFAULT_DATACENTER_BITS: u32 = 5;
    /// 默认序列号位宽。
    pub const DEFAULT_SEQUENCE_BITS: u32 = 12;

    /// 以默认布局（5 + 5 + 12）构建：worker 0、datacenter 0、
    /// [`SequentialSequenceResolver`]、零回拨容忍、`throw` 策略。
    /// 自定义配置走 [`Snowflake::builder`]。
    pub fn builder() -> SnowflakeBuilder {
        SnowflakeBuilder::new()
    }

    /// 生成下一个 Snowflake ID。
    ///
    /// 位布局（LSB 在右）：`| sequence(N) | worker(M) | datacenter(D) | timestamp(63-N-M-D) |`，
    /// 组装式为 `(offset << timestamp_shift) | fixed_bits | sequence`。
    pub fn id(&mut self) -> Result<i64> {
        let mut timestamp = Self::current_time_millis();

        if timestamp < self.last_timestamp {
            let drift = self.last_timestamp - timestamp;
            if drift <= self.clock_tolerance_ms {
                // 回拨在容忍窗口内：沿用上一次的时间戳，绝不倒退发号。
                timestamp = self.last_timestamp;
            } else if self.clock_drift_strategy == ClockDriftStrategy::Wait {
                // 骑过回拨而不是失败：等墙钟追平，以等待预算为界，
                // 严重跑偏的时钟不会把调用方永久卡死。
                let deadline = Self::current_time_millis() + self.clock_drift_wait_ms;
                timestamp = self.wait_next_millis(self.last_timestamp, Some(deadline))?;
            } else {
                return Err(Error::ClockDrift {
                    last_timestamp_ms: self.last_timestamp,
                    current_timestamp_ms: timestamp,
                    tolerance_ms: self.clock_tolerance_ms,
                });
            }
        }

        let mut offset = timestamp - self.epoch;

        if offset < 0 {
            // 时钟早于 epoch：配置错误或大幅回跳。
            return Err(Error::ClockBeforeEpoch {
                epoch_ms: self.epoch,
                current_timestamp_ms: timestamp,
            });
        }
        if offset > self.max_timestamp_offset {
            return Err(Error::TimestampOverflow {
                timestamp_offset: offset,
                max_offset: self.max_timestamp_offset,
            });
        }

        let mut sequence = self.sequence_resolver.next(offset, self.max_sequence)?;

        if sequence.is_none() && timestamp == self.last_timestamp {
            // 本毫秒的 2^sequence_bits 个槽位用尽：等到下一毫秒重试一次。
            timestamp = self.wait_next_millis(self.last_timestamp, None)?;
            offset = timestamp - self.epoch;
            if offset > self.max_timestamp_offset {
                return Err(Error::TimestampOverflow {
                    timestamp_offset: offset,
                    max_offset: self.max_timestamp_offset,
                });
            }
            sequence = self.sequence_resolver.next(offset, self.max_sequence)?;
        }

        let sequence = sequence.ok_or(Error::SequenceUnavailable)?;

        // 全部守卫都通过之后才推进状态：失败的一次调用不会毒化下一次。
        self.last_timestamp = timestamp;

        Ok((offset << self.timestamp_shift) | self.fixed_bits | sequence)
    }

    /// [`Snowflake::id`] 的别名，与 PHP 版 `nextId()` 对应。
    pub fn next_id(&mut self) -> Result<i64> {
        self.id()
    }

    /// 将本实例生成的 ID 反解为各成分（使用本实例的位布局）。
    pub fn parse_id(&self, id: i64) -> ParsedId {
        parse_with_layout(
            id,
            self.epoch,
            self.sequence_bits,
            self.worker_bits,
            self.datacenter_bits,
        )
    }

    /// 用默认位布局（5 + 5 + 12）反解任意 Snowflake ID，
    /// 与 PHP 版静态 `Snowflake::parse($id, $epoch)` 对应。
    pub fn parse(id: i64, epoch: i64) -> ParsedId {
        parse_with_layout(
            id,
            epoch,
            Self::DEFAULT_SEQUENCE_BITS,
            Self::DEFAULT_WORKER_BITS,
            Self::DEFAULT_DATACENTER_BITS,
        )
    }

    /// 某位布局在 epoch 耗尽前能表示的毫秒数。
    ///
    /// 时间戳字段拿走 63 个数据位里剩下的全部：默认布局（5 + 5 + 12 = 22 位）
    /// 余下 41 位，约 69.7 年。给节点或序列号的每一位，都是从寿命里借的。
    pub fn lifespan_ms(worker_bits: u32, datacenter_bits: u32, sequence_bits: u32) -> Result<i64> {
        if worker_bits < 1 || datacenter_bits < 1 || sequence_bits < 1 {
            return Err(Error::InvalidConfig(format!(
                "Bit counts must be at least 1, got worker={worker_bits}, \
                 datacenter={datacenter_bits}, sequence={sequence_bits}."
            )));
        }

        let total_bits = worker_bits + datacenter_bits + sequence_bits;
        if total_bits >= 63 {
            return Err(Error::InvalidConfig(format!(
                "Total worker + datacenter + sequence bits must be less than 63, got {total_bits}."
            )));
        }

        Ok((1i64 << (63 - total_bits)) - 1)
    }

    /// 本实例配置的 epoch（毫秒）。
    pub fn epoch(&self) -> i64 {
        self.epoch
    }

    /// 本实例的 worker ID。
    pub fn worker_id(&self) -> i64 {
        self.worker_id
    }

    /// 本实例的 datacenter ID。
    pub fn datacenter_id(&self) -> i64 {
        self.datacenter_id
    }

    /// 本实例位布局下的最大序列号（`2^sequence_bits - 1`）。
    pub fn max_sequence(&self) -> i64 {
        self.max_sequence
    }

    /// 当前墙钟毫秒时间戳。
    fn current_time_millis() -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis() as i64,
            // 时钟早于 1970 不可达；即使发生，epoch 校验也会把它拦下。
            Err(_) => 0,
        }
    }

    /// 自旋到墙钟越过 `last_timestamp`。
    ///
    /// `deadline_ms` 为绝对墙钟毫秒的放弃点：到点仍落后就报
    /// [`Error::ClockDrift`] 而不是继续等。`None` 表示一直等到追上。
    fn wait_next_millis(&self, last_timestamp: i64, deadline_ms: Option<i64>) -> Result<i64> {
        let mut timestamp = Self::current_time_millis();
        while timestamp <= last_timestamp {
            if let Some(deadline) = deadline_ms
                && timestamp >= deadline
            {
                return Err(Error::ClockDrift {
                    last_timestamp_ms: last_timestamp,
                    current_timestamp_ms: timestamp,
                    tolerance_ms: self.clock_tolerance_ms,
                });
            }
            std::thread::sleep(Duration::from_micros(100)); // 与 PHP 版 usleep(100) 一致
            timestamp = Self::current_time_millis();
        }

        Ok(timestamp)
    }
}

impl Default for Snowflake {
    fn default() -> Self {
        // 默认布局恒合法，unwrap 不可能是运行期错误。
        SnowflakeBuilder::new()
            .build()
            .expect("default layout is always valid")
    }
}

/// 只呈现配置与位宽；序列策略是 trait 对象，不参与打印。
impl fmt::Debug for Snowflake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snowflake")
            .field("epoch", &self.epoch)
            .field("worker_id", &self.worker_id)
            .field("datacenter_id", &self.datacenter_id)
            .field("worker_bits", &self.worker_bits)
            .field("datacenter_bits", &self.datacenter_bits)
            .field("sequence_bits", &self.sequence_bits)
            .field("clock_tolerance_ms", &self.clock_tolerance_ms)
            .field("clock_drift_strategy", &self.clock_drift_strategy)
            .field("clock_drift_wait_ms", &self.clock_drift_wait_ms)
            .finish_non_exhaustive()
    }
}

/// [`Snowflake`] 的构建器 —— 对应 PHP 版构造函数具名参数与 `fromConfig()` 数组。
pub struct SnowflakeBuilder {
    worker_id: i64,
    datacenter_id: i64,
    worker_bits: u32,
    datacenter_bits: u32,
    sequence_bits: u32,
    epoch: Option<i64>,
    sequence_resolver: Option<Box<dyn SequenceResolver>>,
    clock_tolerance_ms: i64,
    clock_drift_strategy: ClockDriftStrategy,
    clock_drift_wait_ms: i64,
}

impl SnowflakeBuilder {
    /// 默认配置：等于 PHP 版 `new Snowflake()` 的全部默认值。
    pub fn new() -> Self {
        Self {
            worker_id: 0,
            datacenter_id: 0,
            worker_bits: Snowflake::DEFAULT_WORKER_BITS,
            datacenter_bits: Snowflake::DEFAULT_DATACENTER_BITS,
            sequence_bits: Snowflake::DEFAULT_SEQUENCE_BITS,
            epoch: None,
            sequence_resolver: None,
            clock_tolerance_ms: 0,
            clock_drift_strategy: ClockDriftStrategy::Throw,
            clock_drift_wait_ms: 1_000,
        }
    }

    /// 工作节点标识，取值范围 `0..=2^worker_bits - 1`。
    pub fn worker_id(mut self, worker_id: i64) -> Self {
        self.worker_id = worker_id;
        self
    }

    /// 数据中心标识，取值范围 `0..=2^datacenter_bits - 1`。
    pub fn datacenter_id(mut self, datacenter_id: i64) -> Self {
        self.datacenter_id = datacenter_id;
        self
    }

    /// worker 位宽，至少 1。
    pub fn worker_bits(mut self, worker_bits: u32) -> Self {
        self.worker_bits = worker_bits;
        self
    }

    /// datacenter 位宽，至少 1。
    pub fn datacenter_bits(mut self, datacenter_bits: u32) -> Self {
        self.datacenter_bits = datacenter_bits;
        self
    }

    /// 序列号位宽，至少 1。
    pub fn sequence_bits(mut self, sequence_bits: u32) -> Self {
        self.sequence_bits = sequence_bits;
        self
    }

    /// 自定义起始时间戳（毫秒），默认 [`Snowflake::DEFAULT_EPOCH`]。
    pub fn epoch(mut self, epoch: i64) -> Self {
        self.epoch = Some(epoch);
        self
    }

    /// 替换序列号策略。
    pub fn sequence_resolver<R: SequenceResolver + 'static>(mut self, resolver: R) -> Self {
        self.sequence_resolver = Some(Box::new(resolver));
        self
    }

    /// 以 trait 对象替换序列号策略（策略在运行期才决定时用）。
    pub fn boxed_sequence_resolver(mut self, resolver: Box<dyn SequenceResolver>) -> Self {
        self.sequence_resolver = Some(resolver);
        self
    }

    /// 允许的时钟回拨上限（毫秒）；0 为严格模式。
    pub fn clock_tolerance_ms(mut self, clock_tolerance_ms: i64) -> Self {
        self.clock_tolerance_ms = clock_tolerance_ms;
        self
    }

    /// 回拨超容忍后的处理策略。
    pub fn clock_drift_strategy(mut self, strategy: ClockDriftStrategy) -> Self {
        self.clock_drift_strategy = strategy;
        self
    }

    /// `wait` 策略放弃前的最长等待（毫秒），必须为正。
    pub fn clock_drift_wait_ms(mut self, clock_drift_wait_ms: i64) -> Self {
        self.clock_drift_wait_ms = clock_drift_wait_ms;
        self
    }

    /// 校验配置并构建生成器 —— 校验顺序与 PHP 版构造函数一致。
    pub fn build(self) -> Result<Snowflake> {
        let Self {
            worker_id,
            datacenter_id,
            worker_bits,
            datacenter_bits,
            sequence_bits,
            epoch,
            sequence_resolver,
            clock_tolerance_ms,
            clock_drift_strategy,
            clock_drift_wait_ms,
        } = self;

        if worker_bits < 1 || datacenter_bits < 1 || sequence_bits < 1 {
            return Err(Error::InvalidConfig(
                "Bit counts must be at least 1.".to_string(),
            ));
        }

        let total_bits = worker_bits + datacenter_bits + sequence_bits;
        if total_bits >= 63 {
            return Err(Error::InvalidConfig(
                "Total worker + datacenter + sequence bits must be less than 63.".to_string(),
            ));
        }

        if clock_drift_wait_ms < 1 {
            return Err(Error::InvalidConfig(format!(
                "Clock drift wait must be a positive number of milliseconds, got {clock_drift_wait_ms}."
            )));
        }

        let timestamp_bits = 63 - total_bits;
        let max_worker_id = (1i64 << worker_bits) - 1;
        let max_datacenter_id = (1i64 << datacenter_bits) - 1;
        let max_sequence = (1i64 << sequence_bits) - 1;

        if worker_id < 0 || worker_id > max_worker_id {
            return Err(Error::InvalidWorkerId {
                worker_id,
                max_worker_id,
            });
        }
        if datacenter_id < 0 || datacenter_id > max_datacenter_id {
            return Err(Error::InvalidDatacenterId {
                datacenter_id,
                max_datacenter_id,
            });
        }

        let worker_shift = sequence_bits;
        let datacenter_shift = sequence_bits + worker_bits;
        let timestamp_shift = sequence_bits + worker_bits + datacenter_bits;
        let fixed_bits = (datacenter_id << datacenter_shift) | (worker_id << worker_shift);
        let max_timestamp_offset = (1i64 << timestamp_bits) - 1;

        Ok(Snowflake {
            epoch: epoch.unwrap_or(Snowflake::DEFAULT_EPOCH),
            worker_id,
            datacenter_id,
            sequence_bits,
            worker_bits,
            datacenter_bits,
            max_sequence,
            timestamp_shift,
            max_timestamp_offset,
            clock_tolerance_ms,
            clock_drift_strategy,
            clock_drift_wait_ms,
            fixed_bits,
            last_timestamp: -1,
            sequence_resolver: sequence_resolver
                .unwrap_or_else(|| Box::new(SequentialSequenceResolver::new())),
        })
    }
}

impl Default for SnowflakeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
