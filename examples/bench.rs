// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 可复现的吞吐基准。
//!
//! ```bash
//! cargo run --release --example bench
//! ```
//!
//! 取 best-of-N 并给出裸时钟基线；结论是 `id()` 的计算速度远超序列号天花板
//! （默认 12 位序列 = 每毫秒 4096 个 = 4.096M/s），实测会贴在天花板上
//! —— 所以另给一行**不撞顶对照**（epoch 前移 + 宽序列位），
//! 把纯生成成本与「等槽位」的时间分开量。

use std::hint::black_box;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use snowflake::{Snowflake, SnowflakeBuilder};

const ITERATIONS: usize = 300_000;
const ROUNDS: usize = 5;

fn main() {
    if cfg!(debug_assertions) {
        eprintln!(
            "⚠ 这是 debug 构建，数字没有意义 —— 请用 `cargo run --release --example bench`\n"
        );
    }

    println!("迭代 {ITERATIONS} 次 × {ROUNDS} 轮，取最优\n");
    println!(
        "{:<36}{:>12}{:>12}{:>10}",
        "操作", "ops/sec", "ns/op", "波动"
    );

    report(
        "SystemTime::now() —— 基线",
        rounds(|| {
            black_box(SystemTime::now());
        }),
    );

    report(
        "SnowflakeBuilder::build()",
        rounds(|| {
            black_box(SnowflakeBuilder::new().build().unwrap());
        }),
    );

    let mut snowflake = Snowflake::default();
    report(
        "id() —— 默认 5+5+12 布局",
        rounds(|| {
            black_box(snowflake.id().unwrap());
        }),
    );

    // 不撞顶的对照：epoch 拨到 60 秒前、序列位开到 20 位（每毫秒 104 万槽位，
    // 30 万次迭代用不完；时间戳剩 33 位 ≈ 99 天）。这一行量的才是纯生成的
    // ns/op —— 上面那行量的是序列槽位天花板。
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut snowflake = SnowflakeBuilder::new()
        .epoch(now_ms - 60_000)
        .sequence_bits(20)
        .build()
        .unwrap();
    report(
        "id() —— 不撞顶对照（20 位序列）",
        rounds(|| {
            black_box(snowflake.id().unwrap());
        }),
    );

    let mut snowflake = Snowflake::default();
    report(
        "id() + parse_id()",
        rounds(|| {
            let id = snowflake.id().unwrap();
            black_box(snowflake.parse_id(id));
        }),
    );

    let per_ms = Snowflake::default().max_sequence() + 1;
    println!(
        "\n序列号天花板：每毫秒 {per_ms} 个 → {:.3}M/s，这是 id() 的物理上限；",
        per_ms as f64 * 1_000.0 / 1_000_000.0
    );
    println!(
        "默认布局那行贴的正是这个天花板（时间花在等下一毫秒的槽位）；\n\
         不撞顶对照行量的才是纯生成成本，两者之差就是「等槽位」的占比。"
    );
}

/// 跑 ROUNDS 轮，每轮 ITERATIONS 次。
fn rounds<F: FnMut()>(mut operation: F) -> Vec<Duration> {
    (0..ROUNDS)
        .map(|_| {
            let started = Instant::now();
            for _ in 0..ITERATIONS {
                operation();
            }
            started.elapsed()
        })
        .collect()
}

/// 打印最优轮的 ops/sec、ns/op 与最差轮相对最优的波动。
fn report(name: &str, durations: Vec<Duration>) {
    let best = *durations.iter().min().unwrap();
    let worst = *durations.iter().max().unwrap();
    let spread = worst.as_secs_f64() / best.as_secs_f64() - 1.0;

    println!(
        "{name:<36}{:>12.0}{:>12.1}{:>9.0}%",
        ITERATIONS as f64 / best.as_secs_f64(),
        best.as_nanos() as f64 / ITERATIONS as f64,
        spread * 100.0
    );
}
