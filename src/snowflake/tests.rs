// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! snowflake.rs 的单元测试：需要访问私有字段（回拨三态、状态推进时机）。

use super::*;
use std::collections::VecDeque;

/// 按剧本吐值的策略：驱动序列耗尽、重试等确定性路径。
struct ScriptedResolver(VecDeque<Option<i64>>);

impl ScriptedResolver {
    fn new(values: impl IntoIterator<Item = Option<i64>>) -> Self {
        Self(values.into_iter().collect())
    }
}

impl SequenceResolver for ScriptedResolver {
    fn next(&mut self, _timestamp_ms: i64, _max_sequence: i64) -> Result<Option<i64>> {
        Ok(self.0.pop_front().unwrap_or(None))
    }
}

/// 把 `last_timestamp` 拨到未来制造回拨 —— 集成测试够不到私有字段，
/// 这里是唯一能确定性驱动三种回拨状态的地方。
fn drifted(strategy: ClockDriftStrategy, tolerance_ms: i64, wait_ms: i64) -> Snowflake {
    let mut s = SnowflakeBuilder::new()
        .clock_drift_strategy(strategy)
        .clock_tolerance_ms(tolerance_ms)
        .clock_drift_wait_ms(wait_ms)
        .build()
        .unwrap();
    // 未来 5 秒：比任何一次调度停顿都大，测试不会因为线程被挂起而「回拨消失」。
    s.last_timestamp = Snowflake::current_time_millis() + 5_000;

    s
}

#[test]
fn backwards_drift_beyond_tolerance_is_rejected() {
    let mut s = drifted(ClockDriftStrategy::Throw, 0, 1_000);

    match s.id() {
        Err(e @ Error::ClockDrift { .. }) => assert!(e.drift_ms().unwrap() > 0),
        other => panic!("期望 ClockDrift，得到 {other:?}"),
    }
}

#[test]
fn backwards_drift_within_tolerance_reuses_last_timestamp() {
    let mut s = SnowflakeBuilder::new()
        .clock_tolerance_ms(1_000)
        .build()
        .unwrap();
    let future = Snowflake::current_time_millis() + 500;
    s.last_timestamp = future;

    let id = s.id().unwrap();

    // 容忍窗口内沿用 last_timestamp 发号 —— 时间戳不倒退。
    assert_eq!(s.parse_id(id).timestamp_ms, future);
}

#[test]
fn wait_strategy_gives_up_after_its_budget() {
    let mut s = drifted(ClockDriftStrategy::Wait, 0, 20);
    let started = std::time::Instant::now();

    match s.id() {
        Err(Error::ClockDrift { .. }) => {}
        other => panic!("期望 ClockDrift，得到 {other:?}"),
    }

    // 50ms 的未来 + 20ms 预算：到点放弃，而不是等满 50ms。
    let waited = started.elapsed();
    assert!(waited < Duration::from_millis(500), "等待过久: {waited:?}");
}

#[test]
fn wait_strategy_rides_out_a_small_drift() {
    let mut s = SnowflakeBuilder::new()
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .clock_drift_wait_ms(1_000)
        .build()
        .unwrap();
    let future = Snowflake::current_time_millis() + 5;
    s.last_timestamp = future;

    let id = s.id().unwrap();

    assert!(s.parse_id(id).timestamp_ms >= future);
}

#[test]
fn failed_call_does_not_poison_the_next_one() {
    let mut s = drifted(ClockDriftStrategy::Throw, 0, 1_000);

    assert!(matches!(s.id(), Err(Error::ClockDrift { .. })));

    // 回拨恢复（清掉人为状态）后，实例照常发号。
    s.last_timestamp = 0;
    assert!(s.id().is_ok());
}

#[test]
fn sequence_exhaustion_waits_for_next_millisecond() {
    let mut s = SnowflakeBuilder::new()
        // 用大容忍窗口把 last_timestamp 钉在「未来」，从而确定性地制造
        // 「同一毫秒用尽」：否则两次调用之间毫秒一跳，就走进另一条分支了。
        .clock_tolerance_ms(60_000)
        .sequence_resolver(ScriptedResolver::new([Some(3), None, Some(9)]))
        .build()
        .unwrap();

    let first = s.id().unwrap();
    assert_eq!(s.parse_id(first).sequence, 3);

    s.last_timestamp = Snowflake::current_time_millis() + 1_000;
    // 第二次调用：timestamp 必等于 last（容忍窗口内沿用）→ 策略给 None
    // → 核心等到下一毫秒重试 → 剧本给 9。
    let second = s.id().unwrap();
    assert_eq!(s.parse_id(second).sequence, 9);
    assert!(second > first);
}

#[test]
fn always_exhausted_resolver_surfaces_sequence_unavailable() {
    let mut s = SnowflakeBuilder::new()
        .sequence_resolver(ScriptedResolver::new([None]))
        .build()
        .unwrap();

    // 契约是「等到下一毫秒重试一次」—— 这里策略永远给 None，重试一次之后
    // 仍拿不到，才报 SequenceUnavailable。
    assert!(matches!(s.id(), Err(Error::SequenceUnavailable)));
    // 状态未被毒化
    assert!(matches!(s.id(), Err(Error::SequenceUnavailable)));
}

#[test]
fn huge_wait_budget_saturates_instead_of_overflowing() {
    // i64::MAX 的等待预算 + debug 构建：修复前 deadline 的加法在这里会 panic
    // （release 下回绕成负值、立刻放弃等待）。saturating_add 后语义 = 一直等到追平。
    let mut s = SnowflakeBuilder::new()
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .clock_drift_wait_ms(i64::MAX)
        .build()
        .unwrap();
    let target = Snowflake::current_time_millis() + 300;
    s.last_timestamp = target;

    let id = s.id().unwrap();

    assert!(s.parse_id(id).timestamp_ms >= target);
}

#[test]
fn resolver_none_on_a_fresh_millisecond_also_retries_next_millisecond() {
    // Redis 这类共享计数器：本进程在这一毫秒的**首次**调用就可能拿到 None
    // （同节点其他进程已把这一毫秒用尽）。「timestamp == last_timestamp」
    // 从来不是契约的一部分 —— 拿到 None 一律等到下一毫秒重试一次。
    let mut s = SnowflakeBuilder::new()
        .sequence_resolver(ScriptedResolver::new([None, Some(5)]))
        .build()
        .unwrap();

    let id = s.id().unwrap();

    assert_eq!(s.parse_id(id).sequence, 5);
}

#[test]
fn strategy_string_roundtrip() {
    assert_eq!(ClockDriftStrategy::default(), ClockDriftStrategy::Throw);
    for s in [ClockDriftStrategy::Throw, ClockDriftStrategy::Wait] {
        assert_eq!(s.as_str().parse::<ClockDriftStrategy>().unwrap(), s);
        assert_eq!(s.to_string(), s.as_str());
    }
    assert!("nope".parse::<ClockDriftStrategy>().is_err());
}
