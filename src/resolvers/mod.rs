// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 内置序列号策略 —— 对应 PHP 版的 `Resolvers\` 命名空间。

mod random;
mod redis;
mod sequential;

pub use random::RandomSequenceResolver;
pub use redis::{RedisClient, RedisSequenceResolver};
pub use sequential::SequentialSequenceResolver;
