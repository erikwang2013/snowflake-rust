// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 基于 Twitter Snowflake 算法的分布式唯一 ID 生成器 —— Rust 移植自 PHP 包
//! [`erikwang2013/snowflake-php`](https://github.com/erikwang2013/snowflake-php)。
//!
//! 无需中心协调节点即可生成 64 位、k-ordered、全局唯一的 ID：每个 ID 由
//! 时间戳、数据中心 ID、工作节点 ID 与序列号组成，单节点每秒上百万个，
//! 没有一次数据库往返。**纯 std、真正零依赖。**
//!
//! ```
//! use snowflake::Snowflake;
//!
//! let mut snowflake = Snowflake::default();
//! let id = snowflake.id()?;                       // 如 508047278033704960
//!
//! let parsed = snowflake.parse_id(id);
//! assert_eq!(parsed.worker_id, 0);
//! # Ok::<(), snowflake::Error>(())
//! ```
//!
//! # 位布局
//!
//! 默认布局（63 数据位 + 1 符号位）：
//!
//! ```text
//! | reserved(1) |  timestamp(41)   | datacenter(5) | worker(5) | sequence(12) |
//! ```
//!
//! 时间戳字段拿走 63 个数据位里剩下的全部 —— 给节点或序列号的每一位都是从
//! 寿命里借的，[`Snowflake::lifespan_ms`] 可以直接算出来。
//!
//! # 并发
//!
//! 生成器是唯一的有状态类型，`id()` 需要 `&mut self`：PHP 版「不要跨协程/线程
//! 共享实例」的警告在这里编译不过。要跨线程共享，用 [`Shared`] ——
//! 它就是 PHP 版七个框架适配器（各自注册共享单例）的共同本质。
//!
//! ```
//! use snowflake::{Shared, Snowflake};
//!
//! let shared = Shared::new(Snowflake::default());
//! std::thread::scope(|s| {
//!     for _ in 0..4 {
//!         let handle = shared.clone();
//!         s.spawn(move || handle.id().unwrap());
//!     }
//! });
//! # Ok::<(), snowflake::Error>(())
//! ```
//!
//! # 配置
//!
//! 代码里用 [`SnowflakeBuilder`]；部署环境用 `SNOWFLAKE_*` 环境变量与
//! [`Snowflake::from_env`]（见 [`config`] 模块的变量表）。
//!
//! # 框架集成
//!
//! Web 服务里把 [`Shared`] 注册为应用状态，处理器按请求取用 ——
//! [`guard::Guard`] 是它在框架语境下的名字，[`integrations`] 为
//! axum / actix-web / rocket / poem / salvo / warp / bee-rust / e-cat
//! 各提供一个 opt-in feature（默认一个都不拉）。
//!
//! # 项目宠物：雪花精灵
//!
//! 一片微笑的雪花 —— 六个分支就是 64 位里分出去的那几段，中心是时间戳。
//! 见 [`pet`]。
//!
//! ```text
//!           *        \   /        *
//!               ------(^_^)------
//!           *        /   \        *
//! ```
//!
//! # 错误
//!
//! 全部失败收敛为 [`Error`] 一枚枚举，与 PHP 版异常体系一一对应（见 [`error`]）。

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod guard;
pub mod integrations;
pub mod parse;
pub mod pet;
pub mod resolver;
pub mod resolvers;
pub mod shared;
pub mod snowflake;

pub use error::{Error, Result};
pub use parse::ParsedId;
pub use resolver::SequenceResolver;
pub use shared::Shared;
pub use snowflake::{ClockDriftStrategy, Snowflake, SnowflakeBuilder};
