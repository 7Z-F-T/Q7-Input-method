//! Q7 输入法 Windows TSF 文本服务 DLL。
//!
//! 架构约束（见 docs/architecture.md）：
//! - 本 DLL 被加载进**每一个**宿主应用进程：必须轻量，且**绝不 panic**
//!   （所有 COM 方法与导出函数入口均经 `panic_guard`）；
//! - 引擎不在本进程运行：按键经命名管道发给 q7-server 处理。

mod edit;
mod factory;
mod guids;
mod ipc_client;
mod panic_guard;
mod registration;
mod text_service;

use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_FAIL, E_POINTER, S_FALSE, S_OK};
use windows::Win32::System::Com::IClassFactory;
use windows::core::{GUID, HRESULT, Interface};

use factory::ClassFactory;
use guids::CLSID_Q7_TEXT_SERVICE;

/// COM 类工厂入口（COM 运行时经此创建文本服务实例）
///
/// # Safety
/// 由 COM 运行时调用，参数为按 COM 约定传入的有效指针；函数内仍做空指针防御。
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    panic_guard::catch(E_FAIL, || unsafe {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }
        ppv.write(std::ptr::null_mut());
        if *rclsid != CLSID_Q7_TEXT_SERVICE {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ClassFactory.into();
        factory.query(riid, ppv)
    })
}

/// 卸载询问：本 DLL 无长期持有的全局对象
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

/// 注册（regsvr32 / 安装程序调用）
#[unsafe(no_mangle)]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    panic_guard::catch(E_FAIL, || match registration::register() {
        Ok(()) => {
            clear_register_error();
            S_OK
        }
        Err(e) => {
            log_register_error("注册", &e);
            e.code()
        }
    })
}

/// 反注册
#[unsafe(no_mangle)]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    panic_guard::catch(E_FAIL, || match registration::unregister() {
        Ok(()) => {
            clear_register_error();
            S_OK
        }
        Err(e) => {
            log_register_error("反注册", &e);
            e.code()
        }
    })
}

/// 注册/反注册失败原因落盘（regsvr32 静默模式下唯一的排查手段）
fn log_register_error(action: &str, e: &windows::core::Error) {
    let msg = format!("Q7-tsf {action}失败: {e} (0x{:08X})\n", e.code().0 as u32);
    panic_guard::debug_string(&msg);
    let _ = std::fs::write(register_error_path(), msg);
}

/// 成功后清理上次的错误记录
fn clear_register_error() {
    let _ = std::fs::remove_file(register_error_path());
}

fn register_error_path() -> std::path::PathBuf {
    std::env::temp_dir().join("q7-tsf-register-error.txt")
}
