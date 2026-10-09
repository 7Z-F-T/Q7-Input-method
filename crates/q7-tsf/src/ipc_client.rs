//! DLL 侧 IPC 客户端。
//!
//! 后台工作线程持有管道连接并负责断线重连（节流）；
//! 前台按键线程以「请求-应答 + 超时」方式交互，**绝不长时间阻塞宿主按键路径**。
//! 服务端不可用时快速失败（返回 `None`），调用方按"按键透传"处理。

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use q7_core::keys::KeyEvent;
use q7_core::protocol::{ClientMsg, KeyResult, PROTOCOL_VERSION, ServerMsg};
use q7_ipc::client::PipeClient;

/// 单次请求超时：超过即视为服务端不可用
const REQUEST_TIMEOUT: Duration = Duration::from_millis(100);
/// 重连节流间隔（避免服务端未启动时每个按键都尝试连接）
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);

struct Command {
    msg: ClientMsg,
    reply: Sender<Option<ServerMsg>>,
}

/// DLL 侧 IPC 客户端（进程内单例，多宿主线程共享）
pub struct IpcClient {
    tx: Mutex<Sender<Command>>,
}

impl IpcClient {
    fn start() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Command>();
        std::thread::Builder::new()
            .name("q7-ipc".into())
            .spawn(move || worker(rx))
            .expect("启动 IPC 工作线程失败");
        Self { tx: Mutex::new(tx) }
    }

    /// 进程内全局单例
    pub fn global() -> &'static IpcClient {
        static CLIENT: std::sync::OnceLock<IpcClient> = std::sync::OnceLock::new();
        CLIENT.get_or_init(IpcClient::start)
    }

    /// 请求-应答；服务端不可用/超时一律返回 `None`
    pub fn request(&self, msg: ClientMsg) -> Option<ServerMsg> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        {
            let tx = self.tx.lock().unwrap_or_else(|e| e.into_inner());
            if tx
                .send(Command {
                    msg,
                    reply: reply_tx,
                })
                .is_err()
            {
                return None;
            }
        }
        match reply_rx.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Some(msg)) => Some(msg),
            _ => None,
        }
    }
}

/// 工作线程：维护连接、断线重连、串行处理请求
fn worker(rx: Receiver<Command>) {
    let mut client: Option<PipeClient> = None;
    let mut last_attempt: Option<Instant> = None;

    for cmd in rx {
        // 未连接：按节流间隔尝试连接 + 握手
        if client.is_none() {
            let now = Instant::now();
            let should_try =
                last_attempt.is_none_or(|t| now.duration_since(t) >= RECONNECT_INTERVAL);
            if should_try {
                last_attempt = Some(now);
                if let Ok(mut c) = PipeClient::connect() {
                    let hello = ClientMsg::Hello {
                        protocol_version: PROTOCOL_VERSION,
                        pid: std::process::id(),
                    };
                    if let Ok(ServerMsg::Welcome { .. }) = c.request::<ClientMsg, ServerMsg>(&hello)
                    {
                        client = Some(c);
                    }
                }
            }
        }

        match client.as_mut() {
            Some(c) => match c.request::<ClientMsg, ServerMsg>(&cmd.msg) {
                Ok(reply) => {
                    let _ = cmd.reply.send(Some(reply));
                }
                Err(_) => {
                    // 连接已断：本次请求失败，丢弃连接，等待下轮节流重连
                    client = None;
                    let _ = cmd.reply.send(None);
                }
            },
            None => {
                let _ = cmd.reply.send(None);
            }
        }
    }
}

/// 发送一次按键；服务端不可用返回 `None`
pub fn send_key(session: u64, event: KeyEvent) -> Option<KeyResult> {
    match IpcClient::global().request(ClientMsg::Key { session, event }) {
        Some(ServerMsg::Key(r)) => Some(r),
        _ => None,
    }
}

/// 会话结束通知（尽力而为）
pub fn end_session(session: u64) {
    let _ = IpcClient::global().request(ClientMsg::SessionEnd { session });
}
