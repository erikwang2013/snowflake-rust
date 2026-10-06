// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! ID 反解 —— 对应 PHP 版的 `parseId()` / `parse()`。
//!
//! `datetime` 成员在 PHP 版里由 `date()` 按**服务器本地时区**格式化；Rust 版
//! 手写 UTC 格式化（零依赖），输出与时区无关 —— 与 PHP README「跨机器对账以
//! `timestamp_ms` 为准」的建议同向，但这里两者都稳定。

/// 一个 Snowflake ID 被反解出的成分，对应 PHP 版 `parseId()` 返回的关联数组。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedId {
    /// 生成时刻的绝对毫秒时间戳（UNIX epoch，与时区无关）。
    pub timestamp_ms: i64,
    /// `timestamp_ms` 的 UTC 渲染，形如 `2025-01-09 00:00:00.123`。
    pub datetime: String,
    /// 工作节点标识。
    pub worker_id: i64,
    /// 数据中心标识。
    pub datacenter_id: i64,
    /// 毫秒内序号。
    pub sequence: i64,
}

/// 按给定位布局反解 ID。实例方法与静态 `parse()` 共用这一份实现。
pub(crate) fn parse_with_layout(
    id: i64,
    epoch: i64,
    sequence_bits: u32,
    worker_bits: u32,
    datacenter_bits: u32,
) -> ParsedId {
    // 位布局（LSB 在右）：| sequence(N) | worker(M) | datacenter(D) | timestamp(63-N-M-D) |
    let max_sequence = (1i64 << sequence_bits) - 1;
    let max_worker_id = (1i64 << worker_bits) - 1;
    let max_datacenter_id = (1i64 << datacenter_bits) - 1;
    let worker_shift = sequence_bits;
    let datacenter_shift = sequence_bits + worker_bits;
    let timestamp_shift = sequence_bits + worker_bits + datacenter_bits;

    let sequence = id & max_sequence;
    let worker_id = (id >> worker_shift) & max_worker_id;
    let datacenter_id = (id >> datacenter_shift) & max_datacenter_id;
    let timestamp_ms = (id >> timestamp_shift) + epoch;

    ParsedId {
        timestamp_ms,
        datetime: format_utc_ms(timestamp_ms),
        worker_id,
        datacenter_id,
        sequence,
    }
}

/// 毫秒时间戳 → `YYYY-MM-DD HH:MM:SS.mmm`（UTC），零依赖。
fn format_utc_ms(timestamp_ms: i64) -> String {
    let seconds = timestamp_ms.div_euclid(1_000);
    let millis = timestamp_ms.rem_euclid(1_000);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let secs_of_day = seconds.rem_euclid(86_400);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{millis:03}",
        secs_of_day / 3_600,
        (secs_of_day % 3_600) / 60,
        secs_of_day % 60,
    )
}

/// 天数（自 1970-01-01 起）→ (年, 月, 日)。
///
/// Howard Hinnant 的 `civil_from_days`，公历、无闰秒、对负数天数是欧几里得语义。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]

    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civils_known_dates() {
        assert_eq!(format_utc_ms(0), "1970-01-01 00:00:00.000");
        assert_eq!(format_utc_ms(1), "1970-01-01 00:00:00.001");
        assert_eq!(format_utc_ms(1_000), "1970-01-01 00:00:01.000");
        assert_eq!(format_utc_ms(86_399_999), "1970-01-01 23:59:59.999");
        assert_eq!(format_utc_ms(86_400_000), "1970-01-02 00:00:00.000");
        // 闰日与年末
        assert_eq!(format_utc_ms(1_709_210_096_789), "2024-02-29 12:34:56.789");
        assert_eq!(format_utc_ms(1_735_689_599_999), "2024-12-31 23:59:59.999");
        // 源项目 README 的示例时间戳（UTC 解读）
        assert_eq!(format_utc_ms(1_736_380_800_123), "2025-01-09 00:00:00.123");
    }

    #[test]
    fn layout_fields_roundtrip_by_hand() {
        // 默认布局：sequence 12 位、worker 5 位、datacenter 5 位
        let epoch = 1_704_067_200_000;
        let offset = 1_736_380_800_123 - epoch;
        let id = (offset << 22) | (3 << 17) | (5 << 12) | 42;

        let parsed = parse_with_layout(id, epoch, 12, 5, 5);
        assert_eq!(
            parsed,
            ParsedId {
                timestamp_ms: 1_736_380_800_123,
                datetime: "2025-01-09 00:00:00.123".to_string(),
                worker_id: 5,
                datacenter_id: 3,
                sequence: 42,
            }
        );
    }
}
