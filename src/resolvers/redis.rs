// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! Redis 共享计数器策略。

use crate::error::Result;
use crate::resolver::SequenceResolver;

/// 鸭子类型的 Redis 客户端：只需提供原子自增与过期两个动作。
///
/// 用 trait 把「鸭子类型」变成编译期契约 —— 不满足的客户端根本传不进来。
/// 客户端由外部注入，因此本 crate 既不依赖 `redis` crate，也不需要任何网络栈
/// （零依赖）。
///
/// 连接失败等故障请在实现里映射为 [`crate::Error::Backend`]：**不要吞掉** ——
/// 静默的失败会发出重复 ID，客户端故障必须直接冒泡。
///
/// 实现应**自带超时**：核心在同步路径上调用它，且调用发生在 [crate::Shared]
/// 的进程级锁内 —— 一次网络阻塞就会占住整把发号锁。部署上，长驻服务优先给
/// 每个进程分配唯一 `(datacenter_id, worker_id)`；本策略留给拿不到稳定身份
/// 的部署做兜底。
pub trait RedisClient: Send {
    /// 原子自增 `key`，返回自增后的计数值。
    fn incr(&mut self, key: &str) -> Result<i64>;

    /// 为 `key` 设置过期秒数；每个 key 通常只在第一次自增后调用一次，
    /// 该次失败后同一毫秒内可能被重试。
    fn expire(&mut self, key: &str, seconds: u64) -> Result<()>;
}

/// 顺序 / 随机两种策略把序列号保存在**进程内存**里，多个进程共用同一个
/// 节点 ID 时可能发出相同的序列号。本策略把计数器放进 Redis —— 共用同一组
/// `(datacenter_id, worker_id)` 的进程从同一个计数器取号。
///
/// 每个毫秒一个 key（`{key_prefix}{时间戳偏移}`），原子自增、各自过期，
/// 繁忙节点也不会无界增长 key 空间。序号零基：某个 key 的第一次自增得到 0。
/// 计数器越过 `max_sequence` 后返回 `None`，核心等到下一毫秒 ——
/// 与 [`crate::resolvers::SequentialSequenceResolver`] 同一契约。TTL 设长也安全：
/// key 存活、计数器继续增长，同一毫秒内的后续调用持续得到 `None`，
/// 直到下一个（由时间戳派生的）key 到来，计数器绝不会在毫秒中途回绕或重启。
///
/// 注意 key_prefix **不按节点隔离**：共用节点 ID 的进程必须共用前缀，
/// 否则等于没共享。
///
/// # 一处资源上界
///
/// `INCR` 与 `EXPIRE` 是两次调用：若 `EXPIRE` 失败（或进程恰在两步之间退出），
/// 该毫秒的 key 便不再带 TTL。首次 `EXPIRE` 失败会记下该毫秒，同一毫秒的
/// 后续调用尽力补发 —— 但补发窗口只覆盖同一毫秒：补发不成或进程退出，该 key
/// 就此永久无 TTL，泄漏上界仍是「每次事故一个 key」（之后每个毫秒都是新 key）。
/// 想让 key 空间严格有界，需要在客户端实现里用 `SET key 0 EX <ttl> NX` +
/// `INCR` 之类的原子写法（本 trait 刻意只留两个最小动作，不扩契约）。
pub struct RedisSequenceResolver<C: RedisClient> {
    client: C,
    key_prefix: String,
    ttl_seconds: u64,
    /// 首次 `EXPIRE` 失败的毫秒（时间戳偏移）—— 同一毫秒的后续调用会补发 TTL。
    pending_ttl_for: Option<i64>,
}

impl<C: RedisClient> RedisSequenceResolver<C> {
    /// 以默认前缀 `snowflake:seq:`、默认 TTL 1 秒构建。
    pub fn new(client: C) -> Self {
        Self {
            client,
            key_prefix: "snowflake:seq:".to_string(),
            ttl_seconds: 1,
            pending_ttl_for: None,
        }
    }

    /// 覆盖 key 前缀（同一 Redis 上多套系统需各用各的命名空间时）。
    pub fn key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    /// 覆盖每个毫秒 key 的过期秒数；低于 1 的取值被钳到 1
    /// （`expire(key, 0)` 会立即删 key，计数器将每次重启）。
    pub fn ttl_seconds(mut self, seconds: u64) -> Self {
        self.ttl_seconds = seconds.max(1);
        self
    }
}

impl<C: RedisClient> SequenceResolver for RedisSequenceResolver<C> {
    fn next(&mut self, timestamp_ms: i64, max_sequence: i64) -> Result<Option<i64>> {
        let key = format!("{}{}", self.key_prefix, timestamp_ms);

        let counter = self.client.incr(&key)?;
        if counter == 1 {
            // 每个 key 的 TTL 只在这里设一次；失败冒泡（fail-closed），但记住它，
            // 同一毫秒的后续调用会补发。
            if let Err(err) = self.client.expire(&key, self.ttl_seconds) {
                self.pending_ttl_for = Some(timestamp_ms);
                return Err(err);
            }
            self.pending_ttl_for = None;
        } else if self.pending_ttl_for == Some(timestamp_ms) {
            // 补发是尽力而为：发号路径（INCR）已成功，TTL 家务失败不再冒泡，
            // 但保留 pending 继续补。
            if self.client.expire(&key, self.ttl_seconds).is_ok() {
                self.pending_ttl_for = None;
            }
        }

        let sequence = counter - 1;

        Ok(if sequence <= max_sequence {
            Some(sequence)
        } else {
            None
        })
    }
}
