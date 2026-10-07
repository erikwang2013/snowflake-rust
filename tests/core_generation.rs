// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 核心生成行为：唯一性、单调性、节点隔离、多线程共享。

use std::collections::HashSet;

use snowflake::{Shared, Snowflake};

#[test]
fn generates_unique_strictly_increasing_ids() {
    let mut snowflake = Snowflake::default();
    let mut seen = HashSet::new();
    let mut previous = i64::MIN;

    for _ in 0..100_000 {
        let id = snowflake.id().unwrap();

        assert!(id > 0, "ID 必须为正（63 数据位 + 1 符号位）: {id}");
        assert!(id > previous, "ID 倒退: {previous} -> {id}");
        assert!(seen.insert(id), "重复 ID: {id}");
        previous = id;
    }

    assert_eq!(seen.len(), 100_000);
}

#[test]
fn next_id_is_an_alias_of_id() {
    let mut snowflake = Snowflake::default();

    let first = snowflake.id().unwrap();
    let second = snowflake.next_id().unwrap();

    assert!(second > first);
}

#[test]
fn separate_nodes_never_collide() {
    let mut worker_one = Snowflake::builder().worker_id(1).build().unwrap();
    let mut worker_two = Snowflake::builder().worker_id(2).build().unwrap();

    let ids_one: HashSet<i64> = (0..20_000).map(|_| worker_one.id().unwrap()).collect();
    let ids_two: HashSet<i64> = (0..20_000).map(|_| worker_two.id().unwrap()).collect();

    assert_eq!(ids_one.len(), 20_000);
    assert_eq!(ids_two.len(), 20_000);
    assert!(ids_one.is_disjoint(&ids_two), "不同节点的 ID 集合相交");
}

#[test]
fn shared_generator_keeps_threads_unique_and_monotonic() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 25_000;

    let shared = Shared::new(Snowflake::default());
    let mut handles = Vec::new();

    for _ in 0..THREADS {
        let handle = shared.clone();
        handles.push(std::thread::spawn(move || {
            (0..PER_THREAD)
                .map(|_| handle.id().unwrap())
                .collect::<Vec<_>>()
        }));
    }

    let mut all = HashSet::new();
    for handle in handles {
        let ids = handle.join().unwrap();

        assert!(
            ids.windows(2).all(|pair| pair[0] < pair[1]),
            "单线程内出现倒退"
        );
        for id in ids {
            assert!(all.insert(id), "并发下出现重复 ID: {id}");
        }
    }

    assert_eq!(all.len(), THREADS * PER_THREAD);
}
