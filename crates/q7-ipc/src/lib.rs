//! 命名管道传输层（TSF DLL ↔ 服务进程）。
//!
//! 帧协议：`u32` LE 长度前缀 + postcard 负载，单帧上限 1 MiB。
//! 同步阻塞 API（PoC 阶段的取舍；引入 Tauri 时再评估 async）。

pub mod client;
pub mod server;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// 管道名（含协议版本，新旧版本互不干扰）
pub const PIPE_NAME: &str = r"\\.\pipe\q7-ime-v0";

/// 单帧上限（防御异常长度）
pub(crate) const MAX_FRAME: usize = 1 << 20;

/// IPC 错误
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("编解码失败: {0}")]
    Codec(#[from] postcard::Error),
    #[error("对端已断开")]
    Disconnected,
    #[error("帧长度非法: {0}")]
    BadFrame(usize),
}

/// 写入一帧（长度前缀 + postcard）
pub(crate) fn write_frame<W, T>(w: &mut W, msg: &T) -> Result<(), IpcError>
where
    W: std::io::Write,
    T: Serialize,
{
    let bytes = postcard::to_stdvec(msg)?;
    if bytes.len() > MAX_FRAME {
        return Err(IpcError::BadFrame(bytes.len()));
    }
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()?;
    Ok(())
}

/// 读取一帧
pub(crate) fn read_frame<R, T>(r: &mut R) -> Result<T, IpcError>
where
    R: std::io::Read,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    read_exact(r, &mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(IpcError::BadFrame(len));
    }
    let mut buf = vec![0u8; len];
    read_exact(r, &mut buf)?;
    Ok(postcard::from_bytes(&buf)?)
}

/// `read_exact`，把 EOF/断管统一映射为 `Disconnected`
fn read_exact<R: std::io::Read>(r: &mut R, buf: &mut [u8]) -> Result<(), IpcError> {
    match r.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e)
            if e.kind() == std::io::ErrorKind::UnexpectedEof
                || e.kind() == std::io::ErrorKind::BrokenPipe =>
        {
            Err(IpcError::Disconnected)
        }
        Err(e) => Err(IpcError::Io(e)),
    }
}
