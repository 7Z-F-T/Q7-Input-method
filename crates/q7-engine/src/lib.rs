//! Q7 输入法拼音引擎。
//!
//! 约束（见 docs/architecture.md）：保持纯 Rust、**零 Windows 依赖**，
//! 为未来跨平台（macOS IMK / Linux fcitx5）留路。

pub mod dict;
pub mod session;

pub use session::Session;
