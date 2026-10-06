// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! `snowflake-rust` 命令行：打印项目宠物、发号、反解。
//!
//! ```text
//! snowflake-rust              打印宠物，发一个 ID
//! snowflake-rust --count 5    发 5 个 ID
//! snowflake-rust --parse ID   反解一个 ID
//! snowflake-rust --mascot     只打印宠物
//! ```
//!
//! 生成器配置读 `SNOWFLAKE_*` 环境变量，与库里的 [`snowflake::Snowflake::from_env`]
//! 完全一致。参数手写解析 —— 一个 CLI banner 不值得一个依赖。

use std::process::ExitCode;

use snowflake::{Snowflake, pet};

const USAGE: &str = "\
用法:
  snowflake-rust              打印宠物，发一个 ID
  snowflake-rust --count N    发 N 个 ID
  snowflake-rust --parse ID   反解一个 ID
  snowflake-rust --mascot     只打印宠物
  snowflake-rust --help       显示本帮助

生成器配置读 SNOWFLAKE_* 环境变量:
  SNOWFLAKE_WORKER_ID, SNOWFLAKE_DATACENTER_ID, SNOWFLAKE_EPOCH, ...";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.as_slice() {
        [] => {
            print_mascot();
            let id = Snowflake::from_env().map_err(|e| e.to_string())?.id();
            println!("\nid: {}", id.map_err(|e| e.to_string())?);
        }
        [flag] if flag == "--mascot" => print_mascot(),
        [flag] if flag == "--help" || flag == "-h" => println!("{USAGE}"),
        [flag, count] if flag == "--count" => {
            let count: usize = count
                .parse()
                .map_err(|_| format!("--count 需要一个正整数，得到 \"{count}\""))?;
            if count == 0 {
                return Err("--count 需要一个正整数，得到 0".to_string());
            }

            let mut snowflake = Snowflake::from_env().map_err(|e| e.to_string())?;
            for _ in 0..count {
                println!("{}", snowflake.id().map_err(|e| e.to_string())?);
            }
        }
        [flag, id] if flag == "--parse" => {
            let id: i64 = id
                .parse()
                .map_err(|_| format!("--parse 需要一个 64 位整数 ID，得到 \"{id}\""))?;

            let snowflake = Snowflake::from_env().map_err(|e| e.to_string())?;
            let parsed = snowflake.parse_id(id);
            println!("id:            {id}");
            println!("timestamp_ms:  {}", parsed.timestamp_ms);
            println!("datetime:      {} (UTC)", parsed.datetime);
            println!("worker_id:     {}", parsed.worker_id);
            println!("datacenter_id: {}", parsed.datacenter_id);
            println!("sequence:      {}", parsed.sequence);
        }
        _ => return Err(format!("无法识别的参数: {}", args.join(" "))),
    }

    Ok(())
}

fn print_mascot() {
    println!("{}", pet::ASCII);
}
