// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 环境变量配置 —— 对应 PHP 版 `SnowflakeFactory::fromEnvironment()` 与
//! 各适配器读取的 `SNOWFLAKE_*` 变量（变量名逐一照搬）。
//!
//! | 环境变量 | 配置键 | 默认值 |
//! |---|---|---|
//! | `SNOWFLAKE_EPOCH` | `epoch` | `1704067200000`（2024-01-01 UTC） |
//! | `SNOWFLAKE_WORKER_ID` | `worker_id` | `0` |
//! | `SNOWFLAKE_DATACENTER_ID` | `datacenter_id` | `0` |
//! | `SNOWFLAKE_WORKER_BITS` | `worker_bits` | `5` |
//! | `SNOWFLAKE_DATACENTER_BITS` | `datacenter_bits` | `5` |
//! | `SNOWFLAKE_SEQUENCE_BITS` | `sequence_bits` | `12` |
//! | `SNOWFLAKE_SEQUENCE_RESOLVER` | `sequence_resolver` | `sequential` |
//! | `SNOWFLAKE_CLOCK_TOLERANCE_MS` | `clock_tolerance_ms` | `0` |
//! | `SNOWFLAKE_CLOCK_DRIFT_STRATEGY` | `clock_drift_strategy` | `throw` |
//! | `SNOWFLAKE_CLOCK_DRIFT_WAIT_MS` | `clock_drift_wait_ms` | `1000` |
//!
//! 与 PHP 版的两处差异，都是 Rust 语言使然：
//!
//! - `SNOWFLAKE_SEQUENCE_RESOLVER` 认内置策略名 `sequential` / `random`。
//!   PHP 收的是类的完整限定名；Rust 不能按名字实例化类型，自定义策略请用
//!   [`SnowflakeBuilder::sequence_resolver`] 注入。
//! - 变量值不是合法整数时报 [`Error::InvalidConfig`]；PHP 的 `fromConfig()`
//!   同样拒绝，但 Laravel 适配器路径上的 `(int)` 转换会把垃圾值静默变 0。
//!
//! 未设置或去空白后为空的变量一律跳过，落到默认值 —— 与 PHP 版
//! 「只转发已设置且非空的变量」一致。

use crate::error::{Error, Result};
use crate::resolvers::{RandomSequenceResolver, SequentialSequenceResolver};
use crate::snowflake::{Snowflake, SnowflakeBuilder};

impl Snowflake {
    /// 从 `SNOWFLAKE_*` 环境变量构建生成器。
    ///
    /// ```no_run
    /// # // SNOWFLAKE_WORKER_ID=7 SNOWFLAKE_DATACENTER_ID=2 ./your-service
    /// let snowflake = snowflake::Snowflake::from_env()?;
    /// # Ok::<(), snowflake::Error>(())
    /// ```
    pub fn from_env() -> Result<Snowflake> {
        SnowflakeBuilder::from_env()?.build()
    }
}

impl SnowflakeBuilder {
    /// 读取 `SNOWFLAKE_*` 环境变量并返回预配置的构建器（尚未校验）。
    pub fn from_env() -> Result<SnowflakeBuilder> {
        let mut builder = SnowflakeBuilder::new();

        if let Some(value) = env("SNOWFLAKE_EPOCH") {
            let epoch = int(&value, "SNOWFLAKE_EPOCH")?;
            if epoch <= 0 {
                return Err(Error::InvalidConfig(format!(
                    "Config \"epoch\" must be a positive integer, got {epoch}."
                )));
            }
            builder = builder.epoch(epoch);
        }
        if let Some(value) = env("SNOWFLAKE_WORKER_ID") {
            builder = builder.worker_id(int(&value, "SNOWFLAKE_WORKER_ID")?);
        }
        if let Some(value) = env("SNOWFLAKE_DATACENTER_ID") {
            builder = builder.datacenter_id(int(&value, "SNOWFLAKE_DATACENTER_ID")?);
        }
        if let Some(value) = env("SNOWFLAKE_WORKER_BITS") {
            builder = builder.worker_bits(bits(&value, "SNOWFLAKE_WORKER_BITS")?);
        }
        if let Some(value) = env("SNOWFLAKE_DATACENTER_BITS") {
            builder = builder.datacenter_bits(bits(&value, "SNOWFLAKE_DATACENTER_BITS")?);
        }
        if let Some(value) = env("SNOWFLAKE_SEQUENCE_BITS") {
            builder = builder.sequence_bits(bits(&value, "SNOWFLAKE_SEQUENCE_BITS")?);
        }
        if let Some(value) = env("SNOWFLAKE_SEQUENCE_RESOLVER") {
            match value.trim().to_ascii_lowercase().as_str() {
                "sequential" => {
                    builder = builder.sequence_resolver(SequentialSequenceResolver::new());
                }
                "random" => builder = builder.sequence_resolver(RandomSequenceResolver::new()),
                other => {
                    return Err(Error::InvalidConfig(format!(
                        "Config \"SNOWFLAKE_SEQUENCE_RESOLVER\" must be \"sequential\" or \
                         \"random\", got \"{other}\"."
                    )));
                }
            }
        }
        if let Some(value) = env("SNOWFLAKE_CLOCK_TOLERANCE_MS") {
            builder = builder.clock_tolerance_ms(int(&value, "SNOWFLAKE_CLOCK_TOLERANCE_MS")?);
        }
        if let Some(value) = env("SNOWFLAKE_CLOCK_DRIFT_STRATEGY") {
            builder = builder.clock_drift_strategy(value.trim().parse()?);
        }
        if let Some(value) = env("SNOWFLAKE_CLOCK_DRIFT_WAIT_MS") {
            let wait = int(&value, "SNOWFLAKE_CLOCK_DRIFT_WAIT_MS")?;
            if wait <= 0 {
                return Err(Error::InvalidConfig(format!(
                    "Config \"clock_drift_wait_ms\" must be a positive integer, got {wait}."
                )));
            }
            builder = builder.clock_drift_wait_ms(wait);
        }

        Ok(builder)
    }
}

/// 读取一个变量；未设置或去空白后为空时返回 `None`。
fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn int(value: &str, name: &str) -> Result<i64> {
    value.trim().parse::<i64>().map_err(|_| {
        Error::InvalidConfig(format!(
            "Config \"{name}\" must be an integer, got \"{value}\"."
        ))
    })
}

fn bits(value: &str, name: &str) -> Result<u32> {
    let number = int(value, name)?;

    u32::try_from(number)
        .ok()
        .filter(|bits| *bits >= 1)
        .ok_or_else(|| {
            Error::InvalidConfig(format!(
                "Config \"{name}\" must be a positive integer, got {number}."
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_and_bits_helpers_validate() {
        assert_eq!(int(" 42 ", "X").unwrap(), 42);
        assert!(matches!(int("nope", "X"), Err(Error::InvalidConfig(_))));

        assert_eq!(bits("12", "X").unwrap(), 12);
        assert!(bits("0", "X").is_err());
        assert!(bits("-3", "X").is_err());
        assert!(bits("99999999999999999999", "X").is_err());
    }

    #[test]
    fn from_env_defaults_hold_when_nothing_is_set() {
        // 本测试二进制不设置任何 SNOWFLAKE_*（真正设变量的场景在集成测试里，
        // 那里能拿 unsafe，且每个测试二进制是独立进程）。
        let snowflake = Snowflake::from_env().unwrap();

        assert_eq!(snowflake.worker_id(), 0);
        assert_eq!(snowflake.datacenter_id(), 0);
        assert_eq!(snowflake.epoch(), Snowflake::DEFAULT_EPOCH);
    }
}
