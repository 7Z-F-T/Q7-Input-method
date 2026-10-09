//! Q7 输入法共享层。
//!
//! 约束（见 docs/architecture.md）：本 crate **不得依赖任何平台 API**，
//! 由 TSF DLL、服务进程、UI 共同使用。

pub mod candidate;
pub mod keys;
pub mod protocol;
