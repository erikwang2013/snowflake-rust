// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 请求守卫：交给处理器的一个共享发号句柄。
//!
//! 框架集成层（axum / actix-web / rocket / poem / salvo / warp / bee-rust / e-cat）
//! 全都在这一层之上做薄适配 —— 把 [`Guard`] 按各自框架的惯例注入请求上下文，
//! 处理器拿到它就能发号，不必把生成器或配置一路透传下去。
//!
//! ```no_run
//! use snowflake::guard::Guard;
//! use snowflake::Snowflake;
//!
//! let guard = Guard::new(Snowflake::default());
//!
//! // 处理器里
//! let id = guard.id()?;
//! let parsed = guard.parse_id(id);
//! # Ok::<(), snowflake::Error>(())
//! ```
//!
//! # Guard 与 [`Shared`](crate::Shared) 是同一个类型
//!
//! 核心侧的「共享壳」[`Shared`](crate::Shared)（`Arc<Mutex<Snowflake>>`）在框架
//! 集成语境里就叫做**请求守卫** —— 这里只是换个角色的名字，没有任何包装：
//!
//! ```text
//! snowflake::Shared       核心视角：多线程共享一个生成器实例
//! snowflake::guard::Guard 框架视角：把它注册进应用状态，处理器按请求取用
//! ```
//!
//! 两个名字指向同一个类型，`Guard::new(...)`、`Guard::from_env()`、`id()`、
//! `parse_id()` 等全部是 [`Shared`](crate::Shared) 的方法。

pub use crate::shared::Shared as Guard;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snowflake;

    /// 别名就是 Shared 本身：两边的方法、克隆、跨线程能力完全一致。
    #[test]
    fn guard_is_shared() {
        let guard: Guard = Guard::new(Snowflake::default());

        let id = guard.id().unwrap();
        assert_eq!(guard.parse_id(id).worker_id, 0);

        let cloned: crate::Shared = guard.clone();
        assert!(cloned.next_id().unwrap() > id, "克隆共享同一实例，必须单调");
    }
}
