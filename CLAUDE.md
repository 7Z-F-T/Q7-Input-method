# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概览

Q7-Input-method 是一个 Windows PC 端输入法，以拼音输入为主。技术栈：Rust 核心 + Tauri 2.12（常驻服务进程 / UI 宿主）+ Vue3 + TypeScript。许可证为 Apache-2.0。

## 常用命令

```bash
cargo build --workspace                  # 构建全部
cargo test --workspace                   # 全部测试（13 个）
cargo test -p q7-engine session          # 运行单个测试（按名字过滤）
cargo clippy --workspace --all-targets   # 静态检查（保持零警告）
cargo fmt --all                          # 格式化
cargo build -p q7-tsf --target i686-pc-windows-msvc   # 32 位 DLL

# 运行 PoC（需要两个终端）：
cargo run -p q7-server                   # 1) 启动引擎服务进程
powershell -ExecutionPolicy Bypass -File scripts\dev-register.ps1   # 2) 注册 DLL（**需管理员**；加 -Unregister 反注册）
```

⚠️ 注册写入 HKLM 机器级（**需管理员**，注册后 Win+Space 可见）。DLL 一旦被宿主进程加载（explorer/IDE 等）就会锁住文件：重编译前需反注册**并重启这些进程**，或改用 `cargo build --target-dir <dir>` 绕过。注册是幂等的（先清后加）。

## 当前状态

阶段 0（TSF PoC）骨架已完成并**真机验证可用**（2026-10-09：注册为系统输入法 → Win+Space 切换 → `nihao`+空格 → 「你好」上屏）：`q7-core`（协议 v0）+ `q7-ipc`（命名管道）+ `q7-engine`（全拼骨架）+ `q7-tsf`（可注册的 TSF DLL，x64/x86）+ `q7-server`（PoC 服务进程）。**候选窗口（Tauri 原型）尚未开始**，这是阶段 0 的剩余项。设计文档在 `docs/`。

## 架构（必读约束）

- 输入法本体是**进程内 TSF COM DLL**（`crates/q7-tsf`，x64 + x86，构建产物 ≤700KB）：只做吞键判断、IPC、组字/上屏；**绝不 panic**（所有 COM 方法与导出函数入口经 `panic_guard::catch`）。
- 引擎**不在 DLL 内**：按键经命名管道（`\\.\pipe\q7-ime-v0`，postcard 帧）发给服务端处理。协议唯一真源在 `crates/q7-core/src/protocol.rs`；不兼容变更须递增 `PROTOCOL_VERSION` 并同步管道名。协议约定：**每个 `ClientMsg` 恰好一个 `ServerMsg` 应答**（请求-应答模型）。
- 所有候选来源按 `CandidateProvider` 理念组织（`Candidate.source` 已含 `Ai`/`Plugin` 预留标记）；`q7-engine` 保持纯 Rust、**零 Windows 依赖**（为跨平台留路）。
- TSF 对文档的写入必须在 `ITfEditSession::DoEditSession` 内执行（`crates/q7-tsf/src/edit.rs`，用同步编辑会话）。
- `#[implement]` 宏细节：获取本对象的 COM 接口指针（如 `ITfKeyEventSink`）只能在 `_Impl` 包装类型的方法里用 `IUnknownImpl::to_interface::<I>(self)`；`TextService` 内部类型上拿不到。`windows-core` 必须作为直接依赖（宏生成代码引用 `windows_core` 路径）。
- 注册写 HKLM 机器级（需管理员）；`registration.rs::com_init` 负责为 DllRegisterServer 线程初始化 COM（regsvr32 不初始化，未初始化会 E_FAIL）；`guids.rs` 中的 GUID 一经发布不可更改。

详见 `docs/architecture.md`，开发顺序见 `docs/roadmap.md`。
