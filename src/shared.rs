// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 共享壳 —— 把生成器注册为应用里的共享单例，收敛成一个类型。
//!
//! 框架集成的心智模型是同一件：**把生成器注册为容器 / 应用状态里的共享
//! 单例**。Rust 里不需要为每个框架各写一个适配器，`Arc<Mutex<Snowflake>>`
//! 一个壳就够 —— `id(&mut self)` 在编译期拦住裸共享：想共享，就必须经过
//! 这个 `Mutex`。
//!
//! ```
//! use snowflake::{Shared, Snowflake};
//!
//! let shared = Shared::new(Snowflake::default());
//! let handle = shared.clone(); // 跨线程分发的只是壳，生成器只有一份
//! let id = handle.id()?;
//! # Ok::<(), snowflake::Error>(())
//! ```

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::error::Result;
use crate::parse::ParsedId;
use crate::snowflake::Snowflake;

/// 可克隆、可跨线程的生成器句柄：`Arc<Mutex<Snowflake>>`。
///
/// 一个进程一份生成器状态（即一份 `(timestamp, sequence)`），
/// 所有线程经由同一把锁取号 —— 共享单例的语义。
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
    ///
    /// 正常路径不阻塞（锁的临界区里只有发号本身）。两种例外：时钟回拨走
    /// `wait` 策略时最多等待 `clock_drift_wait_ms`；本毫秒序列耗尽时睡到
    /// 下一毫秒（亚毫秒级）—— 两者都发生在**持锁状态下**。高并发 async
    /// 服务若把 `wait` 预算配得很大，可考虑把这些调用放进 `spawn_blocking`。
    pub fn id(&self) -> Result<i64> {
        self.lock().id()
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

/// 不锁 Mutex 的 `Debug`：只打印类型名与「状态在锁后」这一事实。
/// 想看生成器配置请用 [`Shared::lock`] 后打印（或直接调用它的 getter）。
///
/// 这样用户的 `#[derive(Debug)] struct AppState { snowflake: Guard, .. }`
/// 能直接编译，日志里顺手 `{:?}` 也不会贸然锁进发号临界区。
impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
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
