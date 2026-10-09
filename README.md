# Q7-Input-method

PC 端输入法（Windows，拼音为主），基于现代项目架构：Rust 核心 + Tauri + Vue3 + TypeScript。

## 特性规划

- 拼音输入：全拼 / 简拼 / 双拼 / 模糊音
- 现代化候选窗口与主题系统
- AI 集成：本地小模型 + 外部大模型 API（联想 / 润色 / 续写）
- WASM 插件系统：候选源、输入方案、主题、工具命令

## 快速开始（开发版 PoC）

前置：Rust 1.99+（stable-msvc）、Visual Studio C++ 生成工具 + Windows SDK。

```bash
cargo build --workspace      # 构建
cargo test --workspace       # 测试
cargo run -p q7-server       # 终端 1：启动引擎服务进程
# 终端 2（管理员）：注册开发版输入法（写 HKLM，需管理员；-Unregister 反注册）
powershell -ExecutionPolicy Bypass -File scripts\dev-register.ps1
```

注册后按 **Win+Space** 切换到「Q7 拼音（开发版）」，在任意应用中输入 `nihao` + 空格即可上屏「你好」。

> ⚠️ 重新编译 `crates/q7-tsf` 前先反注册（DLL 被宿主应用加载时文件被锁）。

## 文档

- [架构设计](docs/architecture.md)
- [开发路线图](docs/roadmap.md)

## 状态

阶段 0（TSF PoC）核心链路已打通并**真机验证可用**：注册为系统输入法后，Win+Space 切换即可用拼音组字/上屏（如 `nihao`+空格 →「你好」）。组成：TSF 文本服务 DLL（x64/x86）+ 命名管道 IPC + 拼音引擎骨架 + PoC 服务进程，13 个自动化测试通过。候选窗口（Tauri 原型）待做。

## 许可证

[Apache-2.0](LICENSE)
