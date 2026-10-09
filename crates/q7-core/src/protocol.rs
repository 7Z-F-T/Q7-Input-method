//! IPC 协议定义（唯一真源）。
//!
//! 版本 v0（PoC）：请求-应答模型，每帧一个消息。
//! 不兼容变更时必须递增 `PROTOCOL_VERSION` 并同步更新管道名（见 q7-ipc）。

use serde::{Deserialize, Serialize};

use crate::candidate::Candidate;
use crate::keys::KeyEvent;

/// IPC 协议版本：DLL 与服务端必须一致
pub const PROTOCOL_VERSION: u32 = 0;

/// DLL → 服务端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// 握手：声明协议版本与客户端进程
    Hello { protocol_version: u32, pid: u32 },
    /// 按键事件（session 标识宿主线内一个文本服务实例）
    Key { session: u64, event: KeyEvent },
    /// 会话结束（TextService 失活时发送）
    SessionEnd { session: u64 },
}

/// 组字状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionState {
    pub text: String,
    /// 光标位置（字符数）
    pub cursor: u32,
}

/// 一次按键的处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyResult {
    /// true = DLL 应吞掉该按键
    pub handled: bool,
    /// Some = 组字进行中（文本需写入宿主文档）；None = 无组字
    pub composition: Option<CompositionState>,
    /// Some = 上屏文本（DLL 先结束组字再提交）
    pub commit: Option<String>,
    /// 当前候选列表（PoC 单页，最多 9 个）
    pub candidates: Vec<Candidate>,
}

impl KeyResult {
    /// 未处理：按键透传给宿主应用
    pub fn ignored() -> Self {
        Self {
            handled: false,
            composition: None,
            commit: None,
            candidates: Vec::new(),
        }
    }
}

/// 服务端 → DLL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        protocol_version: u32,
    },
    Key(KeyResult),
    /// 通用确认（如 SessionEnd 的应答）；协议约定：每个 ClientMsg 恰好一个 ServerMsg
    Ack,
    Error {
        message: String,
    },
}
