// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 序列号策略契约 —— 核心的扩展点，对应 PHP 版的 `Contracts\SequenceResolver`。
//!
//! 核心把每个毫秒的序列号分配完全委托给该 trait，因此更换策略
//! （顺序 / 随机 / Redis / 自定义）无需改动生成器本身。

use crate::error::Result;

/// 序列号策略：为一个毫秒分配下一个序列号。
///
/// 实现必须是 `Send` —— 生成器常被 [`crate::Shared`]（`Arc<Mutex<..>>`）
/// 包装后跨线程使用。
pub trait SequenceResolver: Send {
    /// 申请 `timestamp_ms` 这一毫秒的下一个序列号。
    ///
    /// - `timestamp_ms` —— 时间戳**偏移**（当前毫秒 − epoch），不是裸墙钟；
    ///   Redis 策略直接拿它当 key 的一部分。
    /// - `max_sequence` —— 该位宽允许的最大序列号（`2^sequence_bits - 1`）。
    ///
    /// 返回 `Ok(Some(seq))`（`0..=max_sequence` 之间的下一个序列号）；
    /// `Ok(None)` 表示这一毫秒的序列号已经用尽，核心会等到下一毫秒重试一次；
    /// `Err` 表示策略后端故障（如 Redis 连不上），直接上抛给调用方 ——
    /// 绝不降级为 `None`，否则会静默发出重复 ID。
    fn next(&mut self, timestamp_ms: i64, max_sequence: i64) -> Result<Option<i64>>;
}
