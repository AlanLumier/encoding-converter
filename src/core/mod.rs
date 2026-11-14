//! 核心转换逻辑（与 UI 无关，便于单元测试与 CLI 复用）。

pub mod base_n;
pub mod charset;
pub mod detect;
pub mod escape;
pub mod morse;
pub mod numeral;
pub mod radix;
