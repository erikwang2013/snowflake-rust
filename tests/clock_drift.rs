// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 时钟守卫的公开面：早于 epoch、epoch 耗尽、等待策略配置。
//!
//! 回拨的三种状态（超容忍 throw / 容忍内沿用 / wait 等待与预算放弃）
//! 需要把 `last_timestamp` 拨到未来才能确定性驱动，只有单元测试够得到
//! 私有字段 —— 那几组在 `src/snowflake.rs` 内。

use std::time::{SystemTime, UNIX_EPOCH};

use snowflake::{ClockDriftStrategy, Error, SnowflakeBuilder};

#[test]
fn clock_before_epoch_is_rejected() {
    let future_epoch = now_ms() + 60_000;
    let mut snowflake = SnowflakeBuilder::new().epoch(future_epoch).build().unwrap();

    match snowflake.id() {
        Err(Error::ClockBeforeEpoch {
            epoch_ms,
            current_timestamp_ms,
        }) => {
            assert_eq!(epoch_ms, future_epoch);
            assert!(current_timestamp_ms < future_epoch);
        }
        other => panic!("期望 ClockBeforeEpoch，得到 {other:?}"),
    }
}

#[test]
fn exhausted_epoch_is_rejected() {
    // 30 + 30 + 2 = 62 → 时间戳只剩 1 位：偏移上限 1ms，默认 epoch 早已耗尽。
    let mut snowflake = SnowflakeBuilder::new()
        .worker_bits(30)
        .datacenter_bits(30)
        .sequence_bits(2)
        .build()
        .unwrap();

    match snowflake.id() {
        Err(Error::TimestampOverflow {
            timestamp_offset,
            max_offset,
        }) => {
            assert_eq!(max_offset, 1);
            assert!(timestamp_offset > max_offset);
        }
        other => panic!("期望 TimestampOverflow，得到 {other:?}"),
    }
}

#[test]
fn wait_strategy_configures_and_generates_on_a_healthy_clock() {
    let mut snowflake = SnowflakeBuilder::new()
        .clock_drift_strategy("wait".parse::<ClockDriftStrategy>().unwrap())
        .clock_drift_wait_ms(500)
        .build()
        .unwrap();

    assert!(snowflake.id().is_ok());
}

#[test]
fn wait_budget_must_be_positive() {
    for wait in [0, -5] {
        assert!(
            matches!(
                SnowflakeBuilder::new().clock_drift_wait_ms(wait).build(),
                Err(Error::InvalidConfig(_))
            ),
            "wait={wait} 应被拒绝"
        );
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
