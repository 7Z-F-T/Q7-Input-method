//! FFI 边界 panic 防护。
//!
//! 架构约束（见 docs/architecture.md）：本 DLL 被加载进宿主应用进程，
//! panic 绝不允许跨越 COM/导出函数边界——那会中止宿主应用。
//! 所有 COM 方法与导出函数的入口都必须经 [`catch`]。

use std::panic::{AssertUnwindSafe, catch_unwind};

/// 执行 `f`；发生 panic 时输出调试信息并返回 `fallback`，绝不向外传播。
pub(crate) fn catch<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            debug_string("Q7-tsf: 已捕获 panic（FFI 边界防护）\n");
            fallback
        }
    }
}

/// 输出到调试器（DebugView / VS 输出窗口可见）
pub(crate) fn debug_string(msg: &str) {
    use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { OutputDebugStringW(PCWSTR(wide.as_ptr())) };
}
