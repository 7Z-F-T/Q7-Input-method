//! 管道客户端（TSF DLL 侧使用）。
//!
//! 通过 std 的 `OpenOptions` 打开命名管道路径（Windows 下可用 CreateFile 语义打开），
//! 保持依赖最小、无额外运行时。

use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{IpcError, PIPE_NAME, read_frame, write_frame};

/// 命名管道客户端（同步）
pub struct PipeClient {
    file: File,
}

impl PipeClient {
    /// 连接默认管道；服务端未启动时返回 I/O 错误（调用方负责重试节流）
    pub fn connect() -> Result<Self, IpcError> {
        Self::connect_named(PIPE_NAME)
    }

    pub fn connect_named(name: &str) -> Result<Self, IpcError> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(name)?;
        Ok(Self { file })
    }

    pub fn send<T: Serialize>(&mut self, msg: &T) -> Result<(), IpcError> {
        write_frame(&mut self.file, msg)
    }

    pub fn recv<T: DeserializeOwned>(&mut self) -> Result<T, IpcError> {
        read_frame(&mut self.file)
    }

    /// 发送并等待一个应答（请求-应答模型）
    pub fn request<Rq, Rs>(&mut self, req: &Rq) -> Result<Rs, IpcError>
    where
        Rq: Serialize,
        Rs: DeserializeOwned,
    {
        self.send(req)?;
        self.recv()
    }
}
