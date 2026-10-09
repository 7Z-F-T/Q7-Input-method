//! TIP 注册 / 反注册（由 DllRegisterServer / DllUnregisterServer 调用）。
//!
//! **需要管理员权限**：COM 类写 `HKLM\SOFTWARE\Classes\CLSID`；
//! 语言配置经 `ITfInputProcessorProfileMgr`（内部写 `HKLM\SOFTWARE\Microsoft\CTF\TIP`，
//! 未提权时会返回 E_FAIL）。注册为机器级，发布版安装包沿用同一路径（阶段 2）。

use std::path::PathBuf;

use windows::Win32::Foundation::{E_FAIL, HMODULE, S_OK};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, GUID_TFCAT_TIP_KEYBOARD, ITfCategoryMgr,
    ITfInputProcessorProfileMgr,
};
use windows::core::{Error, GUID, PCWSTR};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::guids::{CLSID_Q7_TEXT_SERVICE, GUID_Q7_PINYIN_PROFILE, LANGID_ZH_CN, PROFILE_DESC};

/// 注册：COM 类 + TIP 语言配置 + 键盘输入法类别
pub fn register() -> windows::core::Result<()> {
    let com_owned = com_init();
    let result = register_inner();
    if com_owned {
        unsafe { CoUninitialize() };
    }
    result
}

fn register_inner() -> windows::core::Result<()> {
    let dll_str = current_dll_path()?.to_string_lossy().to_string();

    // 1. COM 类（HKLM，需管理员）
    register_com_class(&dll_str).map_err(|e| Error::new(E_FAIL, format!("写入注册表失败: {e}")))?;

    // 2. TIP 语言配置 + 类别（机器级；幂等：先移除旧配置，重复注册不会失败）
    unsafe {
        let profiles: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        let desc: Vec<u16> = PROFILE_DESC.encode_utf16().collect();
        // 图标取自 DLL 本体（暂未内置图标资源，系统会使用默认图标）
        let icon: Vec<u16> = dll_str.encode_utf16().collect();
        // 已存在时 RegisterProfile 会以 E_FAIL 拒绝，先清后加保证幂等
        profiles
            .UnregisterProfile(&CLSID_Q7_TEXT_SERVICE, LANGID_ZH_CN, &GUID_Q7_PINYIN_PROFILE, 0)
            .ok();
        profiles.RegisterProfile(
            &CLSID_Q7_TEXT_SERVICE,
            LANGID_ZH_CN,
            &GUID_Q7_PINYIN_PROFILE,
            &desc,
            &icon,
            0,                         // 图标索引
            HKL(std::ptr::null_mut()), // 无替代键盘布局
            0,                         // 首选布局
            true,                      // 默认启用
            0,                         // 标志
        )?;

        let cat: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        cat.UnregisterCategory(
            &CLSID_Q7_TEXT_SERVICE,
            &GUID_TFCAT_TIP_KEYBOARD,
            &CLSID_Q7_TEXT_SERVICE,
        )
        .ok();
        cat.RegisterCategory(
            &CLSID_Q7_TEXT_SERVICE,
            &GUID_TFCAT_TIP_KEYBOARD,
            &CLSID_Q7_TEXT_SERVICE,
        )?;
    }
    Ok(())
}

/// 反注册：撤销语言配置、类别与 COM 类注册
pub fn unregister() -> windows::core::Result<()> {
    let com_owned = com_init();
    let result = unregister_inner();
    if com_owned {
        unsafe { CoUninitialize() };
    }
    result
}

fn unregister_inner() -> windows::core::Result<()> {
    unsafe {
        if let Ok(profiles) = CoCreateInstance::<_, ITfInputProcessorProfileMgr>(
            &CLSID_TF_InputProcessorProfiles,
            None,
            CLSCTX_INPROC_SERVER,
        ) {
            profiles
                .UnregisterProfile(
                    &CLSID_Q7_TEXT_SERVICE,
                    LANGID_ZH_CN,
                    &GUID_Q7_PINYIN_PROFILE,
                    0,
                )
                .ok();
        }
        if let Ok(cat) =
            CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
        {
            cat.UnregisterCategory(
                &CLSID_Q7_TEXT_SERVICE,
                &GUID_TFCAT_TIP_KEYBOARD,
                &CLSID_Q7_TEXT_SERVICE,
            )
            .ok();
        }
    }

    // 清理 COM 类注册（机器级 + 开发早期可能遗留的用户级键）
    let clsid = guid_string(&CLSID_Q7_TEXT_SERVICE);
    let mut last_err: Option<std::io::Error> = None;
    for (hive, prefix) in [
        (HKEY_LOCAL_MACHINE, "SOFTWARE"),
        (HKEY_CURRENT_USER, "Software"),
    ] {
        let key = format!("{prefix}\\Classes\\CLSID\\{clsid}");
        if let Err(e) = RegKey::predef(hive).delete_subkey_all(&key)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            last_err = Some(e);
        }
    }
    match last_err {
        None => Ok(()),
        Some(e) => Err(Error::new(E_FAIL, format!("删除注册表键失败: {e}"))),
    }
}

/// 写 COM 类注册（HKLM\SOFTWARE\Classes\CLSID\{...}，需要管理员权限）
fn register_com_class(dll_path: &str) -> std::io::Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        "SOFTWARE\\Classes\\CLSID\\{}",
        guid_string(&CLSID_Q7_TEXT_SERVICE)
    );
    let (key, _) = hklm.create_subkey(&key_path)?;
    key.set_value("", &"Q7 拼音输入法")?;
    let (inproc, _) = hklm.create_subkey(format!("{key_path}\\InprocServer32"))?;
    inproc.set_value("", &dll_path)?;
    inproc.set_value("ThreadingModel", &"Apartment")?;
    Ok(())
}

/// 取当前 DLL 的完整路径（以本函数地址反查所属模块）
fn current_dll_path() -> windows::core::Result<PathBuf> {
    unsafe {
        let mut module = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(current_dll_path as *const u16),
            &mut module,
        )?;

        // MAX_PATH 可能不够（长路径），直接给足缓冲
        let mut buf = vec![0u16; 32768];
        let len = GetModuleFileNameW(Some(module), &mut buf);
        if len == 0 {
            return Err(Error::new(E_FAIL, "GetModuleFileNameW 调用失败"));
        }
        buf.truncate(len as usize);
        Ok(PathBuf::from(String::from_utf16_lossy(&buf)))
    }
}

/// GUID → 注册表字符串格式 `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`
fn guid_string(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

/// 初始化当前线程的 COM（STA）。
///
/// `DllRegisterServer` 常被 regsvr32 在**未初始化 COM 的线程**上调用，
/// 而注册语言配置需要 `CoCreateInstance`，故必须在此显式初始化。
/// 返回 `true` 表示本次调用完成了初始化，调用方负责配对 `CoUninitialize`。
fn com_init() -> bool {
    // S_OK = 新初始化（需 CoUninitialize）；S_FALSE = 已初始化（不需）；
    // 其他失败（如 RPC_E_CHANGED_MODE）也继续尝试——ProfileMgr 对象可跨套间使用。
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    hr == S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 诊断：验证 `current_dll_path()` 的模块反查逻辑（无需管理员）
    #[test]
    fn diag_current_dll_path() {
        match current_dll_path() {
            Ok(p) => println!("current_dll_path = {p:?}"),
            Err(e) => panic!("current_dll_path 失败: {e} (0x{:08X})", e.code().0 as u32),
        }
    }

    /// 诊断用（手动运行）：真实执行注册各步骤并打印结果。
    ///
    /// ```text
    /// cargo test -p q7-tsf diag_register -- --ignored --nocapture
    /// ```
    ///
    /// 注意：会真实写入注册表（HKLM，**需以管理员身份运行**），指向仓库 `target/debug/q7_tsf.dll`。
    #[test]
    #[ignore = "手动诊断用：会真实写入注册表"]
    fn diag_register() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let dll = std::fs::canonicalize(format!("{manifest}/../../target/debug/q7_tsf.dll"))
            .expect("找不到 DLL，请先 cargo build -p q7-tsf");
        let dll_str = dll.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        println!("DLL 路径: {dll_str}");

        let com_owned = com_init();
        println!("com_init 完成（本函数负责反初始化: {com_owned}）");

        match register_com_class(&dll_str) {
            Ok(()) => println!("[1] register_com_class 成功"),
            Err(e) => println!("[1] register_com_class 失败: {e}"),
        }

        unsafe {
            match CoCreateInstance::<_, ITfInputProcessorProfileMgr>(
                &CLSID_TF_InputProcessorProfiles,
                None,
                CLSCTX_INPROC_SERVER,
            ) {
                Ok(profiles) => {
                    println!("[2] 创建 ProfileMgr 成功");
                    let desc: Vec<u16> = PROFILE_DESC.encode_utf16().collect();
                    let icon: Vec<u16> = dll_str.encode_utf16().collect();
                    match profiles.RegisterProfile(
                        &CLSID_Q7_TEXT_SERVICE,
                        LANGID_ZH_CN,
                        &GUID_Q7_PINYIN_PROFILE,
                        &desc,
                        &icon,
                        0,
                        HKL(std::ptr::null_mut()),
                        0,
                        true,
                        0,
                    ) {
                        Ok(()) => println!("[3] RegisterProfile 成功"),
                        Err(e) => {
                            println!("[3] RegisterProfile 失败: 0x{:08X} {e}", e.code().0 as u32)
                        }
                    }
                }
                Err(e) => println!("[2] 创建 ProfileMgr 失败: 0x{:08X} {e}", e.code().0 as u32),
            }

            match CoCreateInstance::<_, ITfCategoryMgr>(
                &CLSID_TF_CategoryMgr,
                None,
                CLSCTX_INPROC_SERVER,
            ) {
                Ok(cat) => match cat.RegisterCategory(
                    &CLSID_Q7_TEXT_SERVICE,
                    &GUID_TFCAT_TIP_KEYBOARD,
                    &CLSID_Q7_TEXT_SERVICE,
                ) {
                    Ok(()) => println!("[4] RegisterCategory 成功"),
                    Err(e) => {
                        println!("[4] RegisterCategory 失败: 0x{:08X} {e}", e.code().0 as u32)
                    }
                },
                Err(e) => println!("[4] 创建 CategoryMgr 失败: 0x{:08X} {e}", e.code().0 as u32),
            }
        }

        if com_owned {
            unsafe { CoUninitialize() };
        }
    }
}
