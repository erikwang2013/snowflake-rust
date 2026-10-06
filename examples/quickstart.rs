// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 无需框架的完整可运行示例 —— 对应 PHP 版 `docs/examples/plain-php.php`。
//!
//! ```bash
//! cargo run --example quickstart
//! ```

use std::collections::HashSet;

use snowflake::{Shared, Snowflake};

fn main() -> Result<(), snowflake::Error> {
    // 1. 默认配置：worker 0、datacenter 0、默认布局与 epoch。
    let mut snowflake = Snowflake::default();
    let id = snowflake.id()?;
    let parsed = snowflake.parse_id(id);
    println!("默认实例");
    println!("  id:            {id}");
    println!("  datetime(UTC): {}", parsed.datetime);
    println!(
        "  worker={} datacenter={} sequence={}",
        parsed.worker_id, parsed.datacenter_id, parsed.sequence
    );

    // 2. 部署配置：走 SNOWFLAKE_* 环境变量（与 PHP 适配器同一套变量名）。
    let from_env = Snowflake::from_env()?;
    println!(
        "\nfrom_env: worker={} datacenter={}（SNOWFLAKE_WORKER_ID / SNOWFLAKE_DATACENTER_ID）",
        from_env.worker_id(),
        from_env.datacenter_id()
    );

    // 3. 多线程共享：一个进程一份状态，线程之间经 Shared 取号。
    let shared = Shared::new(Snowflake::builder().worker_id(1).datacenter_id(1).build()?);
    println!("\n4 线程 × 3 个 ID:");
    std::thread::scope(|scope| {
        for thread in 0..4 {
            let handle = shared.clone();
            scope.spawn(move || {
                for _ in 0..3 {
                    println!("  thread {thread}: {}", handle.id().unwrap());
                }
            });
        }
    });

    // 4. 自检：同一实例连发 10 万个 ID 必须严格递增且互不相同。
    let mut snowflake = Snowflake::default();
    let mut seen = HashSet::new();
    let mut previous = i64::MIN;
    for _ in 0..100_000 {
        let id = snowflake.id()?;
        assert!(id > previous, "ID 倒退");
        assert!(seen.insert(id), "重复 ID");
        previous = id;
    }
    println!("\n自检通过：10 万个 ID 严格递增、互不相同 ✓");

    Ok(())
}
