// Copyright (c) 2026 erik <erik@erik.xyz> — https://erik.xyz

//! 项目宠物：**雪花精灵**。
//!
//! 本模块不含逻辑，只有形象本身 —— README、CLI banner、下游管理界面共用同一份。
//! 形象文件在 `docs/pet.svg`，终端里用 [`ASCII`]。
//!
//! ```text
//!                   \       /
//!                    \     /
//!            *        \   /        *
//!               ------(^_^)------
//!            *        /   \        *
//!                    /     \
//!                   /       \
//!                 snowflake-rust
//!    64-bit distributed unique ID generator
//! ```
//!
//! 形象自 PHP 版 [`snowflake-php`](https://github.com/erikwang2013/snowflake-php)
//! 原样移植：一片微笑的雪花 —— 六个分支就是 64 位里分出去的那几段，
//! 中心是时间戳。纯装饰：这里没有任何一行碰得到发号逻辑。

/// 项目宠物名。
pub const NAME: &str = "雪花精灵";

/// 一句话人设。
pub const TAGLINE: &str = "64-bit distributed unique ID generator";

/// ASCII 版形象，给终端、日志、CLI banner 用（与 PHP 版 `Snowflake::MASCOT` 一致）。
pub const ASCII: &str = r#"                  \       /
                   \     /
           *        \   /        *
              ------(^_^)------
           *        /   \        *
                   /     \
                  /       \
                snowflake-rust
   64-bit distributed unique ID generator"#;

/// SVG 版形象（`docs/pet.svg`），给 README 与下游界面用。
///
/// 以 `include_str!` 打进库里：零运行时开销，不用就不链接。
/// 也正因如此 `docs/pet.svg` **不能**进 Cargo 的 `exclude` —— 排掉会当场编译失败。
pub const SVG: &str = include_str!("../docs/pet.svg");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_is_bundled_whole() {
        assert!(
            SVG.starts_with("<svg"),
            "SVG 头部: {:?}",
            &SVG[..40.min(SVG.len())]
        );
        assert!(SVG.trim_end().ends_with("</svg>"));
        assert!(SVG.contains(r#"viewBox="0 0 320 340""#), "viewBox 变了");
    }

    /// 形象与库的对账：雪花精灵上那块铭牌必须已经是 Rust 版的名字。
    #[test]
    fn art_carries_the_rust_name() {
        assert!(SVG.contains("snowflake-rust"), "形象上的铭牌没换名");
        assert!(!SVG.contains("snowflake-php"), "还残留 PHP 版名字");
        assert!(SVG.contains("64 位分布式唯一 ID 生成器"), "图注没了");
    }

    #[test]
    fn ascii_and_name_are_not_empty() {
        assert!(!NAME.is_empty() && !TAGLINE.is_empty());
        assert!(ASCII.lines().count() >= 6, "ASCII 形象太短");
        assert!(ASCII.contains("snowflake-rust"));
    }
}
