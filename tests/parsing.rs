// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! ID 反解：往返、静态 parse、UTC datetime。

use std::time::{SystemTime, UNIX_EPOCH};

use snowflake::Snowflake;

#[test]
fn instance_parse_roundtrips_every_component() {
    let mut snowflake = Snowflake::builder()
        .worker_id(5)
        .datacenter_id(3)
        .build()
        .unwrap();

    let before = now_ms();
    let id = snowflake.id().unwrap();
    let after = now_ms();

    let parsed = snowflake.parse_id(id);
    assert_eq!(parsed.worker_id, 5);
    assert_eq!(parsed.datacenter_id, 3);
    assert!((before..=after).contains(&parsed.timestamp_ms));
    // datetime 是 `YYYY-MM-DD HH:MM:SS.mmm`（23 字符）
    assert_eq!(parsed.datetime.len(), 23, "datetime: {}", parsed.datetime);
    assert_eq!(parsed.datetime.as_bytes()[10], b' ');
    assert_eq!(parsed.datetime.as_bytes()[19], b'.');
}

#[test]
fn static_parse_equals_instance_parse_on_default_layout() {
    let mut snowflake = Snowflake::default();
    let id = snowflake.id().unwrap();

    assert_eq!(
        Snowflake::parse(id, Snowflake::DEFAULT_EPOCH),
        snowflake.parse_id(id)
    );
}

#[test]
fn datetime_is_utc_and_matches_the_php_readme_example() {
    // PHP README 的示例 ID：2025-01-09 00:00:00.123 UTC
    let epoch = Snowflake::DEFAULT_EPOCH;
    let timestamp_ms = 1_736_380_800_123i64;
    let id = (timestamp_ms - epoch) << 22; // worker / datacenter / sequence 全 0

    let parsed = Snowflake::parse(id, epoch);

    assert_eq!(parsed.timestamp_ms, timestamp_ms);
    assert_eq!(parsed.datetime, "2025-01-09 00:00:00.123");
    assert_eq!(
        (parsed.worker_id, parsed.datacenter_id, parsed.sequence),
        (0, 0, 0)
    );
}

#[test]
fn custom_epoch_moves_the_timeline() {
    let epoch = 1_700_000_000_000;
    let mut snowflake = Snowflake::builder()
        .epoch(epoch)
        .worker_id(1)
        .build()
        .unwrap();

    let id = snowflake.id().unwrap();
    let parsed = snowflake.parse_id(id);

    assert!(parsed.timestamp_ms >= epoch);
    assert!((parsed.timestamp_ms - now_ms()).abs() < 5_000, "偏离当下过远");
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
