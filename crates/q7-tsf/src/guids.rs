//! Q7 输入法固定 GUID 与常量。
//!
//! ⚠️ 已发布后不可更改（注册表/语言列表以其识别输入法）；开发期修改需先反注册再注册。

use windows::core::GUID;

/// 文本服务 COM 类 ID（本 DLL 实现）
pub const CLSID_Q7_TEXT_SERVICE: GUID = GUID::from_u128(0x7a14d3b1_9c2e_4f6a_8b3d_5e0c1f2a4b60);

/// 拼音输入法语言配置 GUID（未来可注册多个输入方案，各用一个 GUID）
pub const GUID_Q7_PINYIN_PROFILE: GUID = GUID::from_u128(0x7a14d3b2_9c2e_4f6a_8b3d_5e0c1f2a4b61);

/// 语言 ID：中文（简体，中国）
pub const LANGID_ZH_CN: u16 = 0x0804;

/// 语言配置显示名（开发版）
pub const PROFILE_DESC: &str = "Q7 拼音（开发版）";
