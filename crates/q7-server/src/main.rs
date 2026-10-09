//! Q7 输入法 PoC 服务进程（阶段 0）。
//!
//! 职责：命名管道服务 + 每连接会话表 + 用 q7-engine 处理按键。
//! 后续阶段将由 Tauri 宿主进程内嵌（本 crate 变为库，逻辑不变）。

use std::collections::HashMap;

use q7_core::protocol::{ClientMsg, PROTOCOL_VERSION, ServerMsg};
use q7_engine::Session;
use q7_ipc::server::ConnectedPipe;
use tracing::{info, warn};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Q7 输入法 PoC 服务启动，管道: {}", q7_ipc::PIPE_NAME);
    loop {
        match ConnectedPipe::wait_for_client() {
            Ok(pipe) => {
                std::thread::spawn(move || handle_client(pipe));
            }
            Err(e) => {
                warn!("等待客户端失败: {e}");
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
}

/// 单连接处理：Hello 握手 → 按键循环（每连接一张会话表，key = TextService 实例 ID）
fn handle_client(mut pipe: ConnectedPipe) {
    // 握手
    match pipe.recv::<ClientMsg>() {
        Ok(ClientMsg::Hello {
            protocol_version,
            pid,
        }) => {
            if protocol_version != PROTOCOL_VERSION {
                let _ = pipe.send(&ServerMsg::Error {
                    message: format!(
                        "协议版本不匹配: 客户端 {protocol_version}, 服务端 {PROTOCOL_VERSION}"
                    ),
                });
                return;
            }
            info!("客户端已连接: pid={pid}");
            if pipe
                .send(&ServerMsg::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                })
                .is_err()
            {
                return;
            }
        }
        Ok(other) => {
            warn!("首条消息不是 Hello: {other:?}");
            return;
        }
        Err(e) => {
            info!("握手失败: {e}");
            return;
        }
    }

    let mut sessions: HashMap<u64, Session> = HashMap::new();
    loop {
        let msg = match pipe.recv::<ClientMsg>() {
            Ok(m) => m,
            Err(q7_ipc::IpcError::Disconnected) => {
                info!("客户端断开");
                return;
            }
            Err(e) => {
                warn!("接收失败: {e}");
                return;
            }
        };
        match msg {
            ClientMsg::Key { session, event } => {
                let s = sessions.entry(session).or_default();
                let result = s.handle_key(&event);
                if pipe.send(&ServerMsg::Key(result)).is_err() {
                    info!("客户端断开（发送失败）");
                    return;
                }
            }
            ClientMsg::SessionEnd { session } => {
                sessions.remove(&session);
                if pipe.send(&ServerMsg::Ack).is_err() {
                    info!("客户端断开（发送失败）");
                    return;
                }
            }
            ClientMsg::Hello { .. } => {
                warn!("重复的 Hello");
                if pipe
                    .send(&ServerMsg::Error {
                        message: "重复的 Hello".into(),
                    })
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use q7_core::keys::{KeyEvent, KeyState, Modifiers};
    use q7_ipc::client::PipeClient;

    const TEST_PIPE: &str = r"\\.\pipe\q7-ime-test-server";

    fn key(vk: u16) -> KeyEvent {
        KeyEvent {
            vk,
            scan_code: 0,
            modifiers: Modifiers::default(),
            state: KeyState::Down,
        }
    }

    fn letter_vk(ch: char) -> u16 {
        0x41 + (ch as u16 - 'a' as u16)
    }

    /// 端到端（服务端侧）：Hello → 输入 nihao → 空格上屏 → SessionEnd。
    /// 与真实 DLL 完全同一条协议路径，只是按键由测试代码模拟。
    #[test]
    fn server_pipeline_types_pinyin_and_commits() {
        std::thread::spawn(|| {
            if let Ok(pipe) = ConnectedPipe::wait_for_client_named(TEST_PIPE) {
                handle_client(pipe);
            }
        });

        let mut client = {
            let mut c = None;
            for _ in 0..100 {
                if let Ok(x) = PipeClient::connect_named(TEST_PIPE) {
                    c = Some(x);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            c.expect("连接测试管道失败")
        };

        let reply: ServerMsg = client
            .request(&ClientMsg::Hello {
                protocol_version: PROTOCOL_VERSION,
                pid: 0,
            })
            .expect("Hello 往返");
        assert!(matches!(reply, ServerMsg::Welcome { .. }));

        // 依次输入 n i h a o
        let mut last = None;
        for ch in ['n', 'i', 'h', 'a', 'o'] {
            let reply: ServerMsg = client
                .request(&ClientMsg::Key {
                    session: 7,
                    event: key(letter_vk(ch)),
                })
                .expect("按键往返");
            match reply {
                ServerMsg::Key(r) => last = Some(r),
                other => panic!("应为 Key，实际: {other:?}"),
            }
        }
        let r = last.expect("应有按键结果");
        assert!(r.handled);
        assert_eq!(r.composition.as_ref().unwrap().text, "nihao");
        assert_eq!(r.candidates[0].text, "你好");

        // 空格上屏第一个候选
        let reply: ServerMsg = client
            .request(&ClientMsg::Key {
                session: 7,
                event: key(0x20),
            })
            .expect("空格往返");
        match reply {
            ServerMsg::Key(r) => assert_eq!(r.commit.as_deref(), Some("你好")),
            other => panic!("应为 Key，实际: {other:?}"),
        }

        // 会话结束
        let reply: ServerMsg = client
            .request(&ClientMsg::SessionEnd { session: 7 })
            .expect("SessionEnd 往返");
        assert!(matches!(reply, ServerMsg::Ack));
    }
}
