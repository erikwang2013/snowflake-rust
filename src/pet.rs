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
//! 一片微笑的雪花 —— 六个分支就是 64 位里分出去的那几段，
//! 中心是时间戳。纯装饰：这里没有任何一行碰得到发号逻辑。

/// 项目宠物名。
pub const NAME: &str = "雪花精灵";

/// 一句话人设。
pub const TAGLINE: &str = "64-bit distributed unique ID generator";

/// ASCII 版形象，给终端、日志、CLI banner 用。
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

/// 一种表情形态：中文名、emoji 短码与 SVG 源码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Expression {
    /// 中文名，如 `笑`。
    pub label: &'static str,
    /// emoji 短码，如 `:emoji-smile:`。
    pub code: &'static str,
    /// SVG 源码（`docs/pet-expressions/`）。
    pub svg: &'static str,
}

/// 雪花精灵的 12 种表情形态，顺序与 README 表格一致。
///
/// 与 [`SVG`] 一样用 `include_str!` 内联：README 表格、下游表情选择器共用同一份。
pub const EXPRESSIONS: [Expression; 12] = [
    Expression {
        label: "笑",
        code: ":emoji-smile:",
        svg: include_str!("../docs/pet-expressions/smile.svg"),
    },
    Expression {
        label: "欢乐",
        code: ":emoji-joy:",
        svg: include_str!("../docs/pet-expressions/joy.svg"),
    },
    Expression {
        label: "哭",
        code: ":emoji-cry:",
        svg: include_str!("../docs/pet-expressions/cry.svg"),
    },
    Expression {
        label: "沮丧",
        code: ":emoji-depressed:",
        svg: include_str!("../docs/pet-expressions/depressed.svg"),
    },
    Expression {
        label: "惊恐",
        code: ":emoji-terrified:",
        svg: include_str!("../docs/pet-expressions/terrified.svg"),
    },
    Expression {
        label: "愤怒",
        code: ":emoji-angry:",
        svg: include_str!("../docs/pet-expressions/angry.svg"),
    },
    Expression {
        label: "撒娇",
        code: ":emoji-coquettish:",
        svg: include_str!("../docs/pet-expressions/coquettish.svg"),
    },
    Expression {
        label: "卖萌",
        code: ":emoji-cute:",
        svg: include_str!("../docs/pet-expressions/cute.svg"),
    },
    Expression {
        label: "讨好",
        code: ":emoji-pleasing:",
        svg: include_str!("../docs/pet-expressions/pleasing.svg"),
    },
    Expression {
        label: "敲打",
        code: ":emoji-knocking:",
        svg: include_str!("../docs/pet-expressions/knocking.svg"),
    },
    Expression {
        label: "捂脸",
        code: ":emoji-facepalm:",
        svg: include_str!("../docs/pet-expressions/facepalm.svg"),
    },
    Expression {
        label: "吐舌头",
        code: ":emoji-tongue:",
        svg: include_str!("../docs/pet-expressions/tongue.svg"),
    },
];

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

    /// 形象与库的对账：雪花精灵上的铭牌必须与 crate 名一致。
    #[test]
    fn art_carries_the_rust_name() {
        assert!(SVG.contains("snowflake-rust"), "形象上的铭牌不是 crate 名");
        assert!(SVG.contains("64 位分布式唯一 ID 生成器"), "图注没了");
    }

    #[test]
    fn ascii_and_name_are_not_empty() {
        assert!(!NAME.is_empty() && !TAGLINE.is_empty());
        assert!(ASCII.lines().count() >= 6, "ASCII 形象太短");
        assert!(ASCII.contains("snowflake-rust"));
    }

    #[test]
    fn expressions_are_bundled_whole() {
        assert_eq!(EXPRESSIONS.len(), 12);
        for e in EXPRESSIONS {
            assert!(e.svg.starts_with("<svg"), "{} 的 SVG 头部不对", e.label);
            assert!(
                e.svg.trim_end().ends_with("</svg>"),
                "{} 的 SVG 没闭合",
                e.label
            );
            assert!(
                e.svg.contains(r#"viewBox="0 0 320 340""#),
                "{} 的 viewBox 变了",
                e.label
            );
            assert!(!e.label.is_empty(), "表情缺中文名");
            assert!(e.code.starts_with(":emoji-"), "{} 的短码不对", e.label);
        }

        let mut codes: Vec<_> = EXPRESSIONS.iter().map(|e| e.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), EXPRESSIONS.len(), "emoji 短码有重复");
    }
}
