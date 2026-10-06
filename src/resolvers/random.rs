// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 随机起点序列号策略 —— 对应 `RandomSequenceResolver`。

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::resolver::SequenceResolver;

/// 每个毫秒从随机位置开始，随后自增。比顺序策略更难预测，
/// 同时同一毫秒内的 ID 保持单调递增。
///
/// 随机源是内部 splitmix64（`SystemTime` 纳秒异或栈地址播种），
/// **不是密码学安全**的 —— 它只为「打乱起点」，不承担任何安全职责；
/// PHP 版的 `random_int()` 也仅用于同一目的。
pub struct RandomSequenceResolver {
    rng_state: u64,
    last_timestamp: i64,
    sequence: i64,
}

impl RandomSequenceResolver {
    /// 新建策略，随机源以当前时间与进程地址播种。
    pub fn new() -> Self {
        Self {
            rng_state: seed(),
            last_timestamp: i64::MIN,
            sequence: 0,
        }
    }

    /// splitmix64：一轮内联即可用的高质量整型扩散，零依赖。
    fn splitmix64(&mut self) -> u64 {
        self.rng_state = self.rng_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Default for RandomSequenceResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SequenceResolver for RandomSequenceResolver {
    fn next(&mut self, timestamp_ms: i64, max_sequence: i64) -> Result<Option<i64>> {
        if timestamp_ms != self.last_timestamp {
            self.last_timestamp = timestamp_ms;
            // 与 PHP random_int(0, $maxSequence) 一致：起点含 max 本身。
            self.sequence = (self.splitmix64() % (max_sequence as u64 + 1)) as i64;

            return Ok(Some(self.sequence));
        }

        if self.sequence >= max_sequence {
            return Ok(None);
        }

        self.sequence += 1;

        Ok(Some(self.sequence))
    }
}

/// 时间纳秒 + 栈地址（ASLR）—— 两个实例同纳秒创建也能拿到不同种子。
fn seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let stack_marker = 0u8;
    let address = (&stack_marker as *const u8) as u64;

    nanos ^ address.rotate_left(17)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_in_range_then_increments() {
        let mut r = RandomSequenceResolver::new();

        let first = r.next(100, 7).unwrap().unwrap();
        assert!((0..=7).contains(&first), "起点越界: {first}");

        for expected in first + 1..=7 {
            assert_eq!(r.next(100, 7).unwrap(), Some(expected));
        }
        assert_eq!(r.next(100, 7).unwrap(), None); // 用尽

        // 下一毫秒重新摇起点（也可能正好摇到 7，只断言范围）
        let next_ms = r.next(101, 7).unwrap().unwrap();
        assert!((0..=7).contains(&next_ms));
    }

    #[test]
    fn starting_points_actually_vary() {
        let mut r = RandomSequenceResolver::new();
        let starts: std::collections::HashSet<i64> = (0..64)
            .map(|ms| r.next(1_000 + ms, 4095).unwrap().unwrap())
            .collect();

        assert!(starts.len() > 1, "64 次起点完全相同，随机源没在转");
    }
}
