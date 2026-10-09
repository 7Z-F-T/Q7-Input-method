//! 管道服务端：创建命名管道实例，阻塞等待客户端连接。
//!
//! 使用 `CreateNamedPipeW`（windows crate）而非 std，便于控制实例与缓冲区。
//! 接受模型：`wait_for_client` 返回一个已连接的实例，调用方为每个连接开线程后
//! 立即再次调用（一个连接一个实例，天然支持多个宿主应用同时连接）。

use windows::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{PIPE_ACCESS_DUPLEX, ReadFile, WriteFile};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::core::PCWSTR;

use crate::{IpcError, PIPE_NAME, read_frame, write_frame};

const BUF_SIZE: u32 = 64 * 1024;

/// 一个已连接的管道实例（服务端侧）
pub struct ConnectedPipe {
    handle: HANDLE,
}

// SAFETY: 管道句柄仅由持有它的连接线程访问；句柄本身无线程亲和性。
unsafe impl Send for ConnectedPipe {}

impl ConnectedPipe {
    /// 创建默认管道实例并阻塞等待客户端连接（一个"槽位"）
    pub fn wait_for_client() -> Result<Self, IpcError> {
        Self::wait_for_client_named(PIPE_NAME)
    }

    /// 指定管道名的版本（测试用）
    pub fn wait_for_client_named(name: &str) -> Result<Self, IpcError> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                BUF_SIZE,
                BUF_SIZE,
                0,
                None,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let e = unsafe { GetLastError() };
            return Err(IpcError::Io(std::io::Error::from_raw_os_error(e.0 as i32)));
        }

        if unsafe { ConnectNamedPipe(handle, None) }.is_err() {
            // 客户端恰好在 CreateNamedPipe 与 ConnectNamedPipe 之间连上时，
            // ConnectNamedPipe 返回 FALSE 且 GetLastError == ERROR_PIPE_CONNECTED，属正常成功。
            let e = unsafe { GetLastError() };
            if e != ERROR_PIPE_CONNECTED {
                unsafe { CloseHandle(handle) }.ok();
                return Err(IpcError::Io(std::io::Error::from_raw_os_error(e.0 as i32)));
            }
        }
        Ok(Self { handle })
    }

    pub fn send<T: serde::Serialize>(&mut self, msg: &T) -> Result<(), IpcError> {
        write_frame(self, msg)
    }

    pub fn recv<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, IpcError> {
        read_frame(self)
    }
}

impl std::io::Read for ConnectedPipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut read = 0u32;
        match unsafe { ReadFile(self.handle, Some(buf), Some(&mut read), None) } {
            Ok(()) => Ok(read as usize),
            Err(_) => {
                let e = unsafe { GetLastError() };
                Err(std::io::Error::from_raw_os_error(e.0 as i32))
            }
        }
    }
}

impl std::io::Write for ConnectedPipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut written = 0u32;
        match unsafe { WriteFile(self.handle, Some(buf), Some(&mut written), None) } {
            Ok(()) => Ok(written as usize),
            Err(_) => {
                let e = unsafe { GetLastError() };
                Err(std::io::Error::from_raw_os_error(e.0 as i32))
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for ConnectedPipe {
    fn drop(&mut self) {
        unsafe {
            DisconnectNamedPipe(self.handle).ok();
            CloseHandle(self.handle).ok();
        }
    }
}
