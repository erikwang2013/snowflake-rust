// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 可复现的吞吐基准 —— 对应 PHP 版 `scripts/benchmark.php`。
//!
//! ```bash
//! cargo run --release --example bench
//! ```
//!
//! 与 PHP 版同样取 best-of-N 并给出裸时钟基线；不同之处在结论：
//! PHP 是算力先到顶（约 1.6M/s），Rust 的计算速度远超序列号天花板
//! （默认 12 位序列 = 每毫秒 4096 个 = 4.096M/s），`id()` 实测会贴在天花板上。

use std::hint::black_box;
use std::time::{Duration, Instant, SystemTime};

use snowflake::{Snowflake, SnowflakeBuilder};

const ITERATIONS: usize = 300_000;
const ROUNDS: usize = 5;

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("⚠ 这是 debug 构建，数字没有意义 —— 请用 `cargo run --release --example bench`\n");
    }

    println!("迭代 {ITERATIONS} 次 × {ROUNDS} 轮，取最优\n");
    println!(
        "{:<36}{:>12}{:>12}{:>10}",
        "操作", "ops/sec", "ns/op", "波动"
    );

    report("SystemTime::now() —— 基线", rounds(|| {
        black_box(SystemTime::now());
    }));

    report("SnowflakeBuilder::build()", rounds(|| {
        black_box(SnowflakeBuilder::new().build().unwrap());
    }));

    let mut snowflake = Snowflake::default();
    report("id() —— 默认 5+5+12 布局", rounds(|| {
        black_box(snowflake.id().unwrap());
    }));

    let mut snowflake = Snowflake::default();
    report("id() + parse_id()", rounds(|| {
        let id = snowflake.id().unwrap();
        black_box(snowflake.parse_id(id));
    }));

    let per_ms = Snowflake::default().max_sequence() + 1;
    println!(
        "\n序列号天花板：每毫秒 {per_ms} 个 → {:.3}M/s，这是 id() 的物理上限；",
        per_ms as f64 * 1_000.0 / 1_000_000.0
    );
    println!("贴顶说明发号侧已无优化空间，解析与构造才是可比较的计算量。");
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
