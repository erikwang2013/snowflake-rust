// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! Redis 共享计数器策略：用内存假客户端驱动，不依赖真实 Redis。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use snowflake::resolvers::{RedisClient, RedisSequenceResolver};
use snowflake::{Result, SequenceResolver, SnowflakeBuilder};

#[derive(Default)]
struct FakeState {
    counters: HashMap<String, i64>,
    expires: Vec<(String, u64)>,
}

/// 假客户端：多个解析器共享同一份状态 —— 模拟多进程共用一组节点 ID。
#[derive(Clone, Default)]
struct FakeRedis(Arc<Mutex<FakeState>>);

impl FakeRedis {
    fn expires(&self) -> Vec<(String, u64)> {
        self.0.lock().unwrap().expires.clone()
    }
}

impl RedisClient for FakeRedis {
    fn incr(&mut self, key: &str) -> Result<i64> {
        let mut state = self.0.lock().unwrap();
        let counter = state.counters.entry(key.to_string()).or_insert(0);
        *counter += 1;

        Ok(*counter)
    }

    fn expire(&mut self, key: &str, seconds: u64) -> Result<()> {
        self.0.lock().unwrap().expires.push((key.to_string(), seconds));

        Ok(())
    }
}

#[test]
fn shared_counter_prevents_duplicate_sequences() {
    let redis = FakeRedis::default();
    let mut first = RedisSequenceResolver::new(redis.clone());
    let mut second = RedisSequenceResolver::new(redis.clone());

    // 同一毫秒（同一 offset）：两个节点 ID 相同的进程从同一个计数器取号，不重号。
    assert_eq!(first.next(100, 3).unwrap(), Some(0));
    assert_eq!(second.next(100, 3).unwrap(), Some(1));
    assert_eq!(first.next(100, 3).unwrap(), Some(2));
    assert_eq!(second.next(100, 3).unwrap(), Some(3));

    // 越过 max_sequence → 用尽，核心会等到下一毫秒
    assert_eq!(first.next(100, 3).unwrap(), None);

    // 下一毫秒是新的 key，重新从 0 开始
    assert_eq!(first.next(101, 3).unwrap(), Some(0));
}

#[test]
fn ttl_is_armed_once_per_key_with_defaults() {
    let redis = FakeRedis::default();
    let mut resolver = RedisSequenceResolver::new(redis.clone());

    resolver.next(100, 3).unwrap();
    resolver.next(100, 3).unwrap();
    resolver.next(100, 3).unwrap();

    assert_eq!(redis.expires(), vec![("snowflake:seq:100".to_string(), 1)]);
}

#[test]
fn prefix_is_configurable_and_ttl_is_clamped() {
    let redis = FakeRedis::default();
    let mut resolver = RedisSequenceResolver::new(redis.clone())
        .key_prefix("myapp:id:")
        .ttl_seconds(0); // 钳到 1：expire(key, 0) 会立即删 key，计数器将每次重启

    assert_eq!(resolver.next(42, 1).unwrap(), Some(0));

    assert_eq!(redis.expires(), vec![("myapp:id:42".to_string(), 1)]);
}

#[test]
fn generator_end_to_end_with_redis_resolver() {
    let redis = FakeRedis::default();
    let mut snowflake = SnowflakeBuilder::new()
        .worker_id(1)
        .sequence_resolver(RedisSequenceResolver::new(redis.clone()))
        .build()
        .unwrap();

    let first = snowflake.id().unwrap();
    let second = snowflake.id().unwrap();

    assert!(second > first);
    assert_eq!(snowflake.parse_id(first).worker_id, 1);
    assert!(!redis.expires().is_empty(), "TTL 从未被触发");
}
