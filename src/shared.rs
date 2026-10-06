// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 共享壳 —— PHP 版七个框架适配器的共同本质，收敛成一个类型。
//!
//! PHP 版为 Laravel / Yii2 / Yii3 / Webman / ThinkPHP / Hyperf / PSR-11 各写了一个
//! 适配器，它们做的事其实是同一件：**把生成器注册为容器里的共享单例**。
//! Rust 没有七个 DI 容器要迎合，`Arc<Mutex<Snowflake>>` 一个壳就够 ——
//! 而 PHP 版反复警告的「不要跨进程/协程共享实例」，在这里由 `id(&mut self)`
//! 在编译期拦住：想共享，就必须经过这个 `Mutex`。
//!
//! ```
//! use snowflake::{Shared, Snowflake};
//!
//! let shared = Shared::new(Snowflake::default());
//! let handle = shared.clone(); // 跨线程分发的只是壳，生成器只有一份
//! let id = handle.id()?;
//! # Ok::<(), snowflake::Error>(())
//! ```

use std::sync::{Arc, Mutex, MutexGuard};

use crate::error::Result;
use crate::parse::ParsedId;
use crate::snowflake::Snowflake;

/// 可克隆、可跨线程的生成器句柄：`Arc<Mutex<Snowflake>>`。
///
/// 一个进程一份生成器状态（即一份 `(timestamp, sequence)`），
/// 所有线程经由同一把锁取号 —— 这正是 PHP 各适配器注册「共享单例」的语义。
#[derive(Clone)]
pub struct Shared {
    inner: Arc<Mutex<Snowflake>>,
}

impl Shared {
    /// 包装一个生成器。
    pub fn new(snowflake: Snowflake) -> Self {
        Self {
            inner: Arc::new(Mutex::new(snowflake)),
        }
    }

    /// 从 `SNOWFLAKE_*` 环境变量构建共享壳（[`Snowflake::from_env`] 的共享版）。
    pub fn from_env() -> Result<Self> {
        Ok(Self::new(Snowflake::from_env()?))
    }

    /// 生成下一个 ID（等价于对锁后调用 [`Snowflake::id`]）。
    pub fn id(&self) -> Result<i64> {
        self.lock().id()
    }

    /// [`Shared::id`] 的别名。
    pub fn next_id(&self) -> Result<i64> {
        self.lock().next_id()
    }

    /// 反解一个 ID（等价于对锁后调用 [`Snowflake::parse_id`]）。
    pub fn parse_id(&self, id: i64) -> ParsedId {
        self.lock().parse_id(id)
    }

    /// 拿到生成器本体，使用完整 API（调整配置之外的一切操作）。
    ///
    /// 锁中毒时恢复而不是 panic：生成器没有跨字段不变量 —— 持锁者 panic
    /// 最多让 `last_timestamp` 停在过去，而「时钟不倒退」本来就有守卫兜底，
    /// 继续发号是安全的。
    pub fn lock(&self) -> MutexGuard<'_, Snowflake> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl From<Snowflake> for Shared {
    fn from(snowflake: Snowflake) -> Self {
        Self::new(snowflake)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn threads_never_see_the_same_id() {
        let shared = Shared::new(Snowflake::default());
        let mut handles = Vec::new();

        for _ in 0..8 {
            let handle = shared.clone();
            handles.push(std::thread::spawn(move || {
                (0..5_000).map(|_| handle.id().unwrap()).collect::<Vec<_>>()
            }));
        }

        let mut all = HashSet::new();
        for handle in handles {
            let ids = handle.join().unwrap();
            // 单线程内严格递增
            assert!(ids.windows(2).all(|w| w[0] < w[1]), "同一线程内出现倒退");
            for id in ids {
                assert!(all.insert(id), "重复 ID: {id}");
            }
        }

        assert_eq!(all.len(), 8 * 5_000);
    }
}
