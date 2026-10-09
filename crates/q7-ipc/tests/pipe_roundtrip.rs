//! 端到端管道回环测试：真实创建命名管道，验证帧协议与请求-应答。

use q7_core::keys::{KeyEvent, KeyState, Modifiers};
use q7_core::protocol::{ClientMsg, PROTOCOL_VERSION, ServerMsg};
use q7_ipc::client::PipeClient;
use q7_ipc::server::ConnectedPipe;

/// 测试用私有管道名（避免与真实服务干扰）
const TEST_PIPE: &str = r"\\.\pipe\q7-ime-test-roundtrip";

#[test]
fn pipe_roundtrip() {
    let server = std::thread::spawn(|| {
        let mut pipe = ConnectedPipe::wait_for_client_named(TEST_PIPE).expect("创建服务端管道");
        let msg: ClientMsg = pipe.recv().expect("接收 Hello");
        match msg {
            ClientMsg::Hello {
                protocol_version, ..
            } => {
                assert_eq!(protocol_version, PROTOCOL_VERSION)
            }
            other => panic!("首条消息应为 Hello，实际: {other:?}"),
        }
        pipe.send(&ServerMsg::Welcome {
            protocol_version: PROTOCOL_VERSION,
        })
        .expect("发送 Welcome");

        let msg: ClientMsg = pipe.recv().expect("接收按键");
        assert!(matches!(msg, ClientMsg::Key { .. }), "应为 Key 消息");
        pipe.send(&ServerMsg::Key(q7_core::protocol::KeyResult::ignored()))
            .expect("发送按键结果");
    });

    let mut client = connect_with_retry(TEST_PIPE);
    let reply: ServerMsg = client
        .request(&ClientMsg::Hello {
            protocol_version: PROTOCOL_VERSION,
            pid: std::process::id(),
        })
        .expect("Hello 往返");
    assert!(matches!(reply, ServerMsg::Welcome { .. }));

    let ev = KeyEvent {
        vk: 0x41, // 'A'
        scan_code: 0,
        modifiers: Modifiers::default(),
        state: KeyState::Down,
    };
    let reply: ServerMsg = client
        .request(&ClientMsg::Key {
            session: 1,
            event: ev,
        })
        .expect("按键往返");
    assert!(matches!(reply, ServerMsg::Key(_)));

    server.join().expect("服务端线程");
}

fn connect_with_retry(name: &str) -> PipeClient {
    for _ in 0..100 {
        if let Ok(c) = PipeClient::connect_named(name) {
            return c;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("连接测试管道失败: {name}");
}
