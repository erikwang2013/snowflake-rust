// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 顺序序列号策略（默认）—— 对应 `SequentialSequenceResolver`。

use crate::error::Result;
use crate::resolver::SequenceResolver;

/// 经典 Snowflake 行为：每个毫秒序列号从 0 开始顺序递增，
/// 保证单节点内 ID 严格单调递增。
///
/// 游标只保存在本实例内存里 —— 多个进程共用同一个节点 ID 时，
/// 各进程会各自从 0 开始，这正是重复 ID 的来源；那种场景用
/// [`crate::resolvers::RedisSequenceResolver`]。
pub struct SequentialSequenceResolver {
    last_timestamp: i64,
    sequence: i64,
}

impl SequentialSequenceResolver {
    /// 新建策略，游标为空。
    pub fn new() -> Self {
        Self {
            // PHP 版以 PHP_INT_MIN 作哨兵；这里同理，任何真实偏移都不会与之相等。
            last_timestamp: i64::MIN,
            sequence: 0,
        }
    }
}

impl Default for SequentialSequenceResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SequenceResolver for SequentialSequenceResolver {
    fn next(&mut self, timestamp_ms: i64, max_sequence: i64) -> Result<Option<i64>> {
        if timestamp_ms != self.last_timestamp {
            self.last_timestamp = timestamp_ms;
            self.sequence = 0;

            return Ok(Some(self.sequence));
        }

        if self.sequence >= max_sequence {
            return Ok(None);
        }

        self.sequence += 1;

        Ok(Some(self.sequence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_from_zero_each_millisecond() {
        let mut r = SequentialSequenceResolver::new();

        assert_eq!(r.next(100, 3).unwrap(), Some(0));
        assert_eq!(r.next(100, 3).unwrap(), Some(1));
        assert_eq!(r.next(100, 3).unwrap(), Some(2));
        assert_eq!(r.next(100, 3).unwrap(), Some(3));
        assert_eq!(r.next(100, 3).unwrap(), None); // 用尽

        // 下一毫秒重新从 0 开始
        assert_eq!(r.next(101, 3).unwrap(), Some(0));
    }
}
