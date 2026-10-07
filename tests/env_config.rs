// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! `SNOWFLAKE_*` 环境变量（对应 PHP 版 `SnowflakeFactory::fromEnvironment()`）。
//!
//! 环境变量是进程级的，改写在 2024 edition 需要 `unsafe`：整个文件只有这一个
//! 测试在碰它，而且每个集成测试二进制跑在独立进程里，别的测试不受影响。

use snowflake::{Error, Snowflake};

#[test]
fn env_variables_configure_the_generator() {
    // SAFETY: 本进程内没有并发的环境变量读写者（其它测试都不碰 env）。
    unsafe {
        std::env::set_var("SNOWFLAKE_WORKER_ID", "7");
        std::env::set_var("SNOWFLAKE_DATACENTER_ID", "2");
        std::env::set_var("SNOWFLAKE_EPOCH", "1700000000000");
        std::env::set_var("SNOWFLAKE_SEQUENCE_RESOLVER", "RANDOM"); // 大小写不敏感
        std::env::set_var("SNOWFLAKE_CLOCK_TOLERANCE_MS", "5");
    }

    let mut snowflake = Snowflake::from_env().unwrap();
    assert_eq!(snowflake.worker_id(), 7);
    assert_eq!(snowflake.datacenter_id(), 2);
    assert_eq!(snowflake.epoch(), 1_700_000_000_000);
    assert!(snowflake.id().is_ok());

    // 非法值必须指出是哪个变量，而不是静默兜底。
    unsafe { std::env::set_var("SNOWFLAKE_WORKER_ID", "abc") };
    match Snowflake::from_env() {
        Err(Error::InvalidConfig(message)) => {
            assert!(message.contains("SNOWFLAKE_WORKER_ID"), "{message}");
        }
        other => panic!("期望 InvalidConfig，得到 {other:?}"),
    }

    // 数值合法但越界：构建期按 worker 位宽拦下。
    unsafe { std::env::set_var("SNOWFLAKE_WORKER_ID", "999") };
    assert!(matches!(
        Snowflake::from_env(),
        Err(Error::InvalidWorkerId { worker_id: 999, .. })
    ));

    // 空值按未设置处理，落到默认值。
    unsafe {
        std::env::set_var("SNOWFLAKE_WORKER_ID", "");
        std::env::set_var("SNOWFLAKE_SEQUENCE_RESOLVER", "sequential");
        std::env::set_var("SNOWFLAKE_CLOCK_DRIFT_STRATEGY", "wait");
    }
    let snowflake = Snowflake::from_env().unwrap();
    assert_eq!(snowflake.worker_id(), 0);

    // Rust 不能按类名实例化策略：认不出的名字要报错。
    unsafe {
        std::env::set_var(
            "SNOWFLAKE_SEQUENCE_RESOLVER",
            "Erikwang2013\\Snowflake\\Resolvers\\SequentialSequenceResolver",
        )
    };
    assert!(matches!(
        Snowflake::from_env(),
        Err(Error::InvalidConfig(_))
    ));
}
