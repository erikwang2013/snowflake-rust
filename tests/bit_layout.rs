// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 位分配：lifespan_ms、自定义位宽、worker/datacenter 越界。

use std::time::{SystemTime, UNIX_EPOCH};

use snowflake::{Error, Snowflake, SnowflakeBuilder};

#[test]
fn lifespan_matches_the_readme_table() {
    assert_eq!(
        Snowflake::lifespan_ms(5, 5, 12).unwrap(),
        (1i64 << 41) - 1
    );
    assert_eq!(
        Snowflake::lifespan_ms(7, 7, 10).unwrap(),
        (1i64 << 39) - 1
    );
    assert_eq!(
        Snowflake::lifespan_ms(5, 5, 16).unwrap(),
        (1i64 << 37) - 1
    );
    assert_eq!(
        Snowflake::lifespan_ms(5, 5, 20).unwrap(),
        (1i64 << 33) - 1
    );
}

#[test]
fn wider_sequence_shortens_lifespan() {
    let default = Snowflake::lifespan_ms(5, 5, 12).unwrap();
    let wide_sequence = Snowflake::lifespan_ms(5, 5, 16).unwrap();
    let widest = Snowflake::lifespan_ms(5, 5, 20).unwrap();

    assert!(default > wide_sequence && wide_sequence > widest, "寿命表不再单调");
}

#[test]
fn lifespan_rejects_invalid_layouts() {
    for (worker, datacenter, sequence) in [(0, 5, 12), (5, 0, 12), (5, 5, 0), (21, 21, 21)] {
        assert!(
            matches!(
                Snowflake::lifespan_ms(worker, datacenter, sequence),
                Err(Error::InvalidConfig(_))
            ),
            "({worker}, {datacenter}, {sequence}) 应被拒绝"
        );
    }
}

#[test]
fn worker_and_datacenter_ranges_are_enforced() {
    assert_eq!(
        SnowflakeBuilder::new().worker_id(32).build().unwrap_err(),
        Error::InvalidWorkerId {
            worker_id: 32,
            max_worker_id: 31
        }
    );
    assert_eq!(
        SnowflakeBuilder::new()
            .datacenter_id(32)
            .build()
            .unwrap_err(),
        Error::InvalidDatacenterId {
            datacenter_id: 32,
            max_datacenter_id: 31
        }
    );
    assert!(matches!(
        SnowflakeBuilder::new().worker_id(-1).build(),
        Err(Error::InvalidWorkerId { .. })
    ));

    // 10 位 worker：1024 个节点可用，1024 越界
    assert!(SnowflakeBuilder::new().worker_bits(10).worker_id(1023).build().is_ok());
    assert!(matches!(
        SnowflakeBuilder::new().worker_bits(10).worker_id(1024).build(),
        Err(Error::InvalidWorkerId { .. })
    ));
}

#[test]
fn bit_count_limits_are_enforced() {
    assert!(matches!(
        SnowflakeBuilder::new().worker_bits(0).build(),
        Err(Error::InvalidConfig(_))
    ));
    assert!(matches!(
        SnowflakeBuilder::new().datacenter_bits(0).build(),
        Err(Error::InvalidConfig(_))
    ));
    // 30 + 30 + 3 = 63 → 数据位不够
    assert!(matches!(
        SnowflakeBuilder::new()
            .worker_bits(30)
            .datacenter_bits(30)
            .sequence_bits(3)
            .build(),
        Err(Error::InvalidConfig(_))
    ));
    // 30 + 30 + 2 = 62 → 通过（时间戳只剩 1 位是另一回事，构建不拦）
    assert!(SnowflakeBuilder::new()
        .worker_bits(30)
        .datacenter_bits(30)
        .sequence_bits(2)
        .build()
        .is_ok());
}

#[test]
fn custom_layout_roundtrips_through_parse() {
    let mut snowflake = SnowflakeBuilder::new()
        .worker_bits(7)
        .datacenter_bits(7)
        .sequence_bits(10)
        .worker_id(100)
        .datacenter_id(100)
        .build()
        .unwrap();

    let before = now_ms();
    let id = snowflake.id().unwrap();
    let after = now_ms();

    let parsed = snowflake.parse_id(id);
    assert_eq!(parsed.worker_id, 100);
    assert_eq!(parsed.datacenter_id, 100);
    assert!((0..=1023).contains(&parsed.sequence), "10 位序列越界");
    assert!((before..=after).contains(&parsed.timestamp_ms));
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
