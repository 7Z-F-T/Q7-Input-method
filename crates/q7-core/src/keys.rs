//! 按键相关类型：DLL 采集按键事件，经 IPC 传给引擎。

use serde::{Deserialize, Serialize};

/// 按键状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum KeyState {
    Down = 0,
    Up = 1,
}

/// 修饰键组合（位标志）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modifiers(pub u8);

impl Modifiers {
    pub const SHIFT: u8 = 1 << 0;
    pub const CTRL: u8 = 1 << 1;
    pub const ALT: u8 = 1 << 2;
    pub const CAPS_LOCK: u8 = 1 << 3;

    pub fn contains(self, bits: u8) -> bool {
        self.0 & bits == bits
    }

    pub fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }

    pub fn ctrl(self) -> bool {
        self.contains(Self::CTRL)
    }

    pub fn alt(self) -> bool {
        self.contains(Self::ALT)
    }

    pub fn caps_lock(self) -> bool {
        self.contains(Self::CAPS_LOCK)
    }

    pub fn with(self, bits: u8) -> Self {
        Self(self.0 | bits)
    }
}

/// 一次按键事件
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    /// Windows 虚拟键码（VK_*）
    pub vk: u16,
    /// 扫描码（保留：部分宿主应用场景需要）
    pub scan_code: u16,
    pub modifiers: Modifiers,
    pub state: KeyState,
}
