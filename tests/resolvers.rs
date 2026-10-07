// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 序列号策略：自定义策略注入、策略后端故障上抛、随机策略端到端。

use std::collections::{HashSet, VecDeque};

use snowflake::resolvers::{RandomSequenceResolver, SequentialSequenceResolver};
use snowflake::{Error, Result, SequenceResolver, SnowflakeBuilder};

/// 按剧本吐值的策略（对应 PHP 测试套件的 FixedSequenceResolver）：
/// 驱动确定性路径 —— 包括「本毫秒用尽 → 下一毫秒重试」。
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

#[test]
fn custom_resolver_drives_the_sequence() {
    let mut snowflake = SnowflakeBuilder::new()
        .sequence_resolver(ScriptedResolver::new([Some(7), Some(8)]))
        .build()
        .unwrap();

    let first = snowflake.id().unwrap();
    let second = snowflake.id().unwrap();

    assert_eq!(snowflake.parse_id(first).sequence, 7);
    assert_eq!(snowflake.parse_id(second).sequence, 8);
    assert!(second > first);
}

#[test]
fn boxed_resolver_works_too() {
    let resolver: Box<dyn SequenceResolver> = Box::new(SequentialSequenceResolver::new());
    let mut snowflake = SnowflakeBuilder::new()
        .boxed_sequence_resolver(resolver)
        .build()
        .unwrap();

    assert!(snowflake.id().is_ok());
}

#[test]
fn resolver_backend_failure_surfaces_instead_of_being_swallowed() {
    struct FailingResolver;

    impl SequenceResolver for FailingResolver {
        fn next(&mut self, _timestamp_ms: i64, _max_sequence: i64) -> Result<Option<i64>> {
            Err(Error::Backend("redis 连接失败".to_string()))
        }
    }

    let mut snowflake = SnowflakeBuilder::new()
        .sequence_resolver(FailingResolver)
        .build()
        .unwrap();

    // 静默吞掉后端故障会发重复 ID —— 必须原样上抛给调用方。
    match snowflake.id() {
        Err(Error::Backend(message)) => assert!(message.contains("redis")),
        other => panic!("期望 Backend，得到 {other:?}"),
    }
}

#[test]
fn random_strategy_end_to_end_stays_inside_the_layout() {
    let mut snowflake = SnowflakeBuilder::new()
        .sequence_resolver(RandomSequenceResolver::new())
        .build()
        .unwrap();

    let mut seen = HashSet::new();
    let mut previous = i64::MIN;

    for _ in 0..10_000 {
        let id = snowflake.id().unwrap();
        assert!(id > previous);
        assert!(seen.insert(id));
        previous = id;

        let parsed = snowflake.parse_id(id);
        assert!(
            (0..=4095).contains(&parsed.sequence),
            "序列越界: {}",
            parsed.sequence
        );
    }
}
