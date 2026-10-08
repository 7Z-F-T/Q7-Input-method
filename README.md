# Q7-Input-method

PC 端输入法（Windows，拼音为主），基于现代项目架构：Rust 核心 + Tauri + Vue3 + TypeScript。

## 特性规划

- 拼音输入：全拼 / 简拼 / 双拼 / 模糊音
- 现代化候选窗口与主题系统
- AI 集成：本地小模型 + 外部大模型 API（联想 / 润色 / 续写）
- WASM 插件系统：候选源、输入方案、主题、工具命令

## 文档

- [架构设计](docs/architecture.md)
- [开发路线图](docs/roadmap.md)

## 状态

早期设计阶段，尚无可运行代码。开发从路线图的阶段 0（TSF PoC）开始。

## 许可证

[Apache-2.0](LICENSE)
