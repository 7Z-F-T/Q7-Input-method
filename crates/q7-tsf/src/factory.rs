//! COM 类工厂：向 COM/TSF 运行时暴露 TextService。

use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::core::{GUID, Interface, Ref, Result, implement};

use crate::panic_guard::catch;
use crate::text_service::TextService;

/// Q7 文本服务的类工厂
#[implement(IClassFactory)]
pub struct ClassFactory;

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        catch(Err(E_FAIL.into()), || {
            // 不支持聚合
            if !punkouter.is_null() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            if riid.is_null() || ppvobject.is_null() {
                return Err(E_POINTER.into());
            }
            unsafe { ppvobject.write(std::ptr::null_mut()) };

            let service: windows::core::IUnknown = TextService::create().into();
            let hr = unsafe { service.query(riid, ppvobject) };
            hr.ok()
        })
    }

    fn LockServer(&self, _flock: windows::core::BOOL) -> Result<()> {
        // 本 DLL 无长期全局状态，锁定/解锁无操作
        Ok(())
    }
}
