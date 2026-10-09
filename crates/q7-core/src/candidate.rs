//! 候选词类型。`CandidateSource` 为 AI / 插件候选预留标记。

use serde::{Deserialize, Serialize};

/// 候选词来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CandidateSource {
    /// 主词典
    Dict = 0,
    /// 用户词库
    User = 1,
    /// AI 模型（预留）
    Ai = 2,
    /// 插件（预留）
    Plugin = 3,
}

/// 单个候选词
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub text: String,
    /// 注释（拼音提示、来源说明等）
    pub comment: Option<String>,
    pub source: CandidateSource,
}
