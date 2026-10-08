# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概览

Q7-Input-method 是一个 Windows PC 端输入法，以拼音输入为主。技术栈：Rust 核心 + Tauri 2.12（常驻服务进程 / UI 宿主）+ Vue3 + TypeScript。许可证为 Apache-2.0。

## 当前状态（重要）

仓库处于起步阶段：尚无源码与构建系统，设计文档在 `docs/`：

- `docs/architecture.md` —— 总体架构（TSF DLL / 服务进程 / 候选窗口分工）、目录结构、技术选型、插件与 AI 预留设计
- `docs/roadmap.md` —— 分阶段开发计划（从阶段 0 TSF PoC 起步）

- 不要臆造或预写任何尚未在仓库中真实存在的命令；代码落地后在本文件补充真实的构建、运行、测试命令（含如何运行单个测试）；
- 保持本文件与实际仓库状态同步是本文件的第一要务。

## 架构（必读约束）

输入法本体是**进程内 TSF COM DLL**（`crates/q7-tsf`，x64 + x86），引擎不在 DLL 内：DLL 只做吞键判断、IPC、组字/上屏、光标矩形获取，且绝不 panic（所有 COM 入口 `catch_unwind`）。引擎、词典、AI、插件运行在 Tauri 常驻进程（`apps/q7-desktop` + `crates/q7-server`）。所有候选来源统一走 `CandidateProvider` trait 管线（主词典 / 用户词库 / AI / 插件都只是 Provider）。`q7-engine` 保持纯 Rust、零 Windows 依赖（为跨平台留路）。详见 `docs/architecture.md`。
