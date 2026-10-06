// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 错误类型 —— 对应 PHP 版的异常体系（`SnowflakeException` 基类 + 五个子类）。
//!
//! PHP 用「异常基类 + 子类」表达六种失败；Rust 收敛为一个枚举，每个变体保留
//! 原异常携带的上下文字段，[`Display`](std::fmt::Display) 文案与原异常逐字对齐，
//! 便于两边日志对账。

use std::fmt;

/// Snowflake 的全部错误，对应 PHP 版异常体系：
///
/// | PHP 异常 | Rust 变体 |
/// |---|---|
/// | `SnowflakeException`（基类） | `Error` 本身 |
/// | `ClockDriftException`（回拨超容忍） | [`Error::ClockDrift`] |
/// | `ClockDriftException`（时钟早于 epoch） | [`Error::ClockBeforeEpoch`] |
/// | `TimestampOverflowException` | [`Error::TimestampOverflow`] |
/// | `InvalidWorkerIdException` | [`Error::InvalidWorkerId`] |
/// | `InvalidDatacenterIdException` | [`Error::InvalidDatacenterId`] |
/// | `\InvalidArgumentException`（配置校验） | [`Error::InvalidConfig`] |
/// | `\RuntimeException`（序列耗尽） | [`Error::SequenceUnavailable`] |
/// | 客户端异常冒泡（Redis 故障等） | [`Error::Backend`] |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 系统时钟回拨超过容忍值（`clock_tolerance_ms`）。
    ClockDrift {
        /// 上一次发号使用的墙钟毫秒时间戳。
        last_timestamp_ms: i64,
        /// 当前墙钟毫秒时间戳。
        current_timestamp_ms: i64,
        /// 配置的回拨容忍值（毫秒）。
        tolerance_ms: i64,
    },
    /// 系统时钟早于配置的 epoch —— 配置错误或时钟大幅倒退，
    /// PHP 版同样抛出 `ClockDriftException`（走自定义消息分支），这里单列变体。
    ClockBeforeEpoch {
        /// 配置的 epoch（毫秒）。
        epoch_ms: i64,
        /// 当前墙钟毫秒时间戳。
        current_timestamp_ms: i64,
    },
    /// 时间戳偏移超出该位分配能表示的上限，epoch 已耗尽。
    TimestampOverflow {
        /// 当前时间戳偏移（毫秒）。
        timestamp_offset: i64,
        /// 该位分配允许的最大偏移。
        max_offset: i64,
    },
    /// Worker ID 超出 `2^worker_bits - 1`（或为负）。
    InvalidWorkerId {
        /// 配置的 worker ID。
        worker_id: i64,
        /// 该位宽允许的最大值。
        max_worker_id: i64,
    },
    /// Datacenter ID 超出 `2^datacenter_bits - 1`（或为负）。
    InvalidDatacenterId {
        /// 配置的 datacenter ID。
        datacenter_id: i64,
        /// 该位宽允许的最大值。
        max_datacenter_id: i64,
    },
    /// 配置非法：位宽越界、策略取值错误、环境变量不是合法整数等。
    InvalidConfig(String),
    /// 序列号耗尽且推进到下一毫秒后仍拿不到 —— 发号速率超过了节点上限。
    SequenceUnavailable,
    /// 序列策略的后端故障（如 Redis 连接失败），由策略实现上报。
    /// 不降级、不吞掉 —— 静默的失败会发出重复 ID；PHP 版让客户端异常直接冒泡，
    /// 与此同一取向。
    Backend(String),
}

impl Error {
    /// 回拨的毫秒数（`last - current`），仅 [`Error::ClockDrift`] 有意义。
    pub fn drift_ms(&self) -> Option<i64> {
        match self {
            Error::ClockDrift {
                last_timestamp_ms,
                current_timestamp_ms,
                ..
            } => Some(last_timestamp_ms - current_timestamp_ms),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // 与 ClockDriftException 的默认文案逐字对齐。
            Error::ClockDrift {
                last_timestamp_ms,
                current_timestamp_ms,
                tolerance_ms,
            } => write!(
                f,
                "System clock moved backwards by {} ms (last: {}, current: {}). Tolerance: {} ms.",
                last_timestamp_ms - current_timestamp_ms,
                last_timestamp_ms,
                current_timestamp_ms,
                tolerance_ms
            ),
            // 与 ClockDriftException 的自定义消息分支对齐。
            Error::ClockBeforeEpoch {
                epoch_ms,
                current_timestamp_ms,
            } => write!(
                f,
                "System clock is before the configured epoch (epoch: {}, current: {}).",
                epoch_ms, current_timestamp_ms
            ),
            Error::TimestampOverflow {
                timestamp_offset,
                max_offset,
            } => write!(
                f,
                "Timestamp offset {} exceeds maximum {}. \
                 The epoch has been exhausted; choose a more recent epoch.",
                timestamp_offset, max_offset
            ),
            Error::InvalidWorkerId {
                worker_id,
                max_worker_id,
            } => write!(
                f,
                "Worker ID {} exceeds maximum {} (2^bits - 1). Check worker_bits configuration.",
                worker_id, max_worker_id
            ),
            Error::InvalidDatacenterId {
                datacenter_id,
                max_datacenter_id,
            } => write!(
                f,
                "Datacenter ID {} exceeds maximum {} (2^bits - 1). \
                 Check datacenter_bits configuration.",
                datacenter_id, max_datacenter_id
            ),
            Error::InvalidConfig(message) => f.write_str(message),
            Error::SequenceUnavailable => f.write_str(
                "Unable to obtain sequence number. Try reducing ID generation rate.",
            ),
            Error::Backend(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

/// `Result` 别名，错误固定为 [`Error`]。
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_drift_message_matches_php_exception() {
        let e = Error::ClockDrift {
            last_timestamp_ms: 1_700_000_000_100,
            current_timestamp_ms: 1_700_000_000_000,
            tolerance_ms: 5,
        };
        assert_eq!(
            e.to_string(),
            "System clock moved backwards by 100 ms (last: 1700000000100, \
             current: 1700000000000). Tolerance: 5 ms."
        );
        assert_eq!(e.drift_ms(), Some(100));
        assert_eq!(Error::SequenceUnavailable.drift_ms(), None);
    }

    #[test]
    fn every_variant_implements_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&Error::SequenceUnavailable);
        assert_error(&Error::InvalidConfig("x".into()));
    }
}
