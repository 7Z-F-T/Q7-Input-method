# Q7 输入法架构设计

Q7-Input-method 是一个 Windows PC 端输入法（拼音为主），技术栈为 Rust + Tauri + Vue3 + TypeScript，面向未来的 AI 能力集成与插件生态。

> 本文回答"怎么搭"；开发顺序见 [`roadmap.md`](roadmap.md)。

## 1. 架构总览

### 1.1 核心结论：Tauri 不做输入法本体

Windows 输入法本体必须是 **TSF（Text Services Framework）文本服务**——一个进程内 COM DLL，会被加载进**每一个**目标应用进程（Chrome、Office、VS Code…）。它必须是几百 KB 级、x64 + x86 双架构、绝不 panic 的轻量组件。

Tauri/WebView2 承担三个角色：**常驻服务进程（引擎宿主）+ 候选窗口渲染 + 设置界面**。

### 1.2 进程视图

```
┌─ 宿主应用进程 (Chrome / VS Code / Office / 任意程序) ─────────┐
│  q7-tsf.dll  (进程内 COM，轻量，x64 + x86)                    │
│    ITfTextInputProcessorEx / ITfKeyEventSink / 组字管理        │
│    只做: 吞键判断 · 发送按键 · 应用组字/上屏 · 上报光标矩形      │
└───────────────────┬───────────────────────────────────────────┘
                    │ 命名管道 (postcard 编码, 会话 ID)
┌───────────────────▼───────────────────────────────────────────┐
│  Q7 常驻进程 (apps/q7-desktop, Tauri 2.12, 单实例, 开机自启)    │
│  ├─ q7-server   会话管理 · 按键管线 · 向 DLL 下发指令           │
│  ├─ q7-engine   拼音引擎 (切分/词图/打分/联想，纯 Rust 跨平台)   │
│  ├─ q7-dict     主词典 mmap + 用户词库 SQLite                  │
│  ├─ q7-ai       本地小模型 + 云端 LLM (provider trait)         │
│  ├─ q7-plugin   wasmtime 插件宿主 (预留)                       │
│  ├─ 候选窗口    Tauri 透明窗口, 预创建+预热, WS_EX_NOACTIVATE   │
│  └─ 设置窗口/托盘 (Vue3 界面)                                  │
└───────────────────────────────────────────────────────────────┘
```

### 1.3 数据流（一次按键的完整旅程）

1. 宿主应用中按下按键
2. TSF DLL 的 `ITfKeyEventSink::OnTestKeyDown` 判断是否吞键（中文模式吞、英文直通）
3. 吞键 → 通过命名管道发送按键事件（含会话 ID、上下文信息）
4. Server 交给 `q7-engine` 计算：音节切分 → 词图 → **CandidateProvider 管线**（主词典 / 用户词库 / AI / 插件）→ 打分排序
5. 返回指令：更新组字（由 DLL 应用到宿主）+ 候选列表
6. Server 通知候选窗口显示并定位（光标矩形由 DLL 经 `ITfContextView::GetTextExt` 获取后上报，失败时回退 `GetGUIThreadInfo`）
7. 用户选词/回车 → 上屏指令 → DLL 提交文本、结束组字

### 1.4 关键设计决策（后改代价极大，必须遵守）

1. **引擎不进 DLL**：词典若加载进每个进程，内存爆炸且无法共享用户学习数据。DLL 只做 TSF 接口 + IPC。
2. **组字状态在 DLL、引擎状态在 Server**（TSF 要求组字由文本服务持有），两者靠 IPC 协议同步 → 这是最需要稳定的协议，阶段 0 就冻结。
3. **所有候选来源走 `CandidateProvider` trait 管线**（主词典 / 用户词库 / AI / 插件都只是 Provider）——这是"插件与 AI 预留"的落实点，阶段 1 起就按此实现，阶段 3/4 接入时零重构。
4. **`q7-engine` 不依赖任何 Windows API**（纯 Rust）→ 未来跨平台（macOS IMK / Linux fcitx5）只换壳。

## 2. 目录结构

```
Q7-Input-method/
├── Cargo.toml                      # Rust workspace 根
├── rust-toolchain.toml             # 固定 MSRV（Tauri 2.12 要求 ≥1.90）
├── docs/
│   ├── architecture.md            # 本文档
│   ├── roadmap.md                 # 分阶段开发计划
│   ├── ipc-protocol.md            # DLL ↔ Server 协议（版本化，阶段 0 建立）
│   └── plugin-api.md              # 插件 ABI / 权限 / 生命周期（阶段 4 前补齐）
├── crates/
│   ├── q7-core/                   # 共享层：无平台依赖
│   │   ├── config.rs              # 全局配置模型（serde，UI/引擎共用）
│   │   ├── protocol.rs            # IPC 消息定义（唯一真源）
│   │   ├── candidate.rs           # 候选词结构（含来源标记/AI标记）
│   │   └── keys.rs                # 按键、键码、修饰键
│   ├── q7-dict/                   # 数据层
│   │   ├── builder/               # dict-src → .q7dict 编译工具
│   │   ├── loader.rs              # mmap 加载 + FST/Trie 查询
│   │   └── user_db.rs             # SQLite 用户词库/频次/学习
│   ├── q7-engine/                 # 拼音引擎（纯逻辑，无 Windows 依赖）
│   │   ├── decoder/               # 全拼/简拼/双拼/模糊音 → 音节切分
│   │   ├── segment/               # 词图 DAG 构建
│   │   ├── providers/             # CandidateProvider trait + 内置实现
│   │   ├── scorer/                # 打分重排（词频 → n-gram → AI rerank）
│   │   └── session.rs             # 输入会话状态机（翻页/取消/上屏）
│   ├── q7-ipc/                    # 命名管道传输 + 帧协议
│   ├── q7-tsf/                    # (cdylib) TSF 文本服务 DLL
│   │   ├── text_service.rs        # ITfTextInputProcessorEx
│   │   ├── key_sink.rs            # ITfKeyEventSink（OnTestKeyDown 吞键）
│   │   ├── composition.rs         # 组字/光标矩形(GetTextExt + 回退)
│   │   ├── registration.rs        # DllRegisterServer / TIP 注册
│   │   └── panic_guard.rs         # 所有 COM 入口 catch_unwind（必须！）
│   ├── q7-server/                 # 引擎服务（库，由 Tauri 进程内嵌）
│   │   ├── session_mgr.rs         # 每宿主进程/线程的会话
│   │   ├── pipeline.rs            # 按键 → Provider 管线 → 指令
│   │   └── window_ctrl.rs         # 候选窗定位/显示（WebView 窗口控制）
│   ├── q7-ai/                     # AI（预留）
│   │   ├── provider.rs            # AiProvider trait（Reranker / Generator）
│   │   ├── local/                 # n-gram → ONNX(ort) / Candle 小模型
│   │   ├── remote/                # OpenAI 兼容 API（reqwest）
│   │   └── privacy.rs             # 敏感上下文检测/脱敏/黑白名单
│   └── q7-plugin/                 # 插件系统（预留）
│       ├── host.rs                # wasmtime 沙箱 / 配额 / 隔离
│       ├── manifest.rs            # plugin.toml 解析与校验
│       └── registry.rs            # 发现/启停/配置/日志
├── sdk/                           # 插件开发者 SDK（独立于内部 crate）
│   ├── plugin-api.wit             # WIT 接口定义（ABI 契约，先定）
│   ├── rust/  typescript/  python/  # 三种语言的 PDK 模板
├── apps/
│   └── q7-desktop/                # Tauri 应用（唯一常驻进程）
│       ├── src-tauri/
│       │   ├── main.rs            # 启动 server、IPC、窗口、托盘
│       │   ├── commands.rs        # tauri commands（设置/词典/插件/AI）
│       │   └── tauri.conf.json
│       └── ui/                    # Vue3 + TS + Vite（多入口）
│           ├── index.html         # 设置窗口入口
│           ├── candidate.html     # 候选窗口入口（透明/无边框）
│           ├── src/settings/      # 设置页（路由 + Pinia）
│           ├── src/candidate/     # 候选条渲染
│           ├── src/shared/        # ts-rs/specta 从 Rust 生成的类型 + IPC 客户端
│           └── vite.config.ts     # rollupOptions 多页构建
├── plugins/                       # 官方示例插件（emoji / 日期 / 计算器）
├── data/
│   ├── dict-src/                  # 原始词库/词频 + 许可说明（逐条记录！）
│   ├── dict-build/                # 编译产物 .q7dict
│   └── themes/                    # 内置主题（CSS/JSON）
├── scripts/
│   ├── build.ps1                  # x64 + x86 构建、词典编译、前端构建
│   ├── register.ps1               # 开发模式 TIP 注册/注销
│   └── package.ps1                # NSIS 打包（含注册、自启、卸载）
└── tests/                         # 兼容性矩阵清单 + 端到端脚本
```

## 3. 技术选型

| 层 | 选型 | 备注 |
|---|---|---|
| TSF 绑定 | `windows` crate (windows-rs) | 参考 imekit / khiin-rs / keymagic-3 的 COM 写法 |
| IPC | 命名管道 + `postcard` | 后置优化可换共享内存；DLL 侧同步最小依赖 |
| 服务运行时 | `tokio` | 仅 server 侧；DLL 不用 async runtime |
| 词典 | 自研二进制格式 + `memmap2` + FST | 编译工具独立于运行时 |
| 用户数据 | `rusqlite` (SQLite) | 词库记忆/频次/自造词 |
| UI | Tauri 2.12 + Vue3 + TS + Vite 多入口 | `ts-rs` / `tauri-specta` 同步类型；Tauri updater 自动更新 |
| 插件 | `wasmtime` + Component Model (WIT) | Extism 可作为多语言 PDK 的备选封装 |
| AI 本地 | 先 n-gram（零依赖）→ `ort` (ONNX) 或 `candle` | int8 量化，CPU 推理 |
| AI 云端 | `reqwest` + OpenAI 兼容协议 | 一套协议兼容 DeepSeek/通义/智谱/Ollama |
| 打包 | NSIS（Tauri bundler）+ 自定义注册脚本 | **代码签名必须做**（EV 证书，否则 SmartScreen/杀软拦截） |

## 4. 插件系统设计（预留）

- **扩展点（trait，阶段 1 内部实现）**：候选源（`CandidateProvider`）、上屏过滤器（`CommitFilter`）、输入方案（`Encoder`）、主题（`ThemeProvider`）、工具命令（触发词如 `:smile`）。
- **契约先行**：`sdk/plugin-api.wit` 从阶段 1 就写好（哪怕宿主未实现），manifest 格式先定：

```toml
# %APPDATA%\Q7\plugins\<id>\plugin.toml
id = "com.example.emoji"
version = "0.1.0"
api_version = "1"
[extensions]  # 声明挂钩点
candidate_provider = true
[permissions] # 默认全禁，显式申请
network = false
filesystem = false
```

- **沙箱**：wasmtime fuel / 超时 / 内存限额；插件 panic 或超时只影响它自己的候选产出，绝不拖垮引擎（沙箱验证用例进测试）。
- **插件目录**：`%APPDATA%\Q7\plugins\<id>\`，含 `plugin.toml` + `plugin.wasm`。
- **管理 UI**：设置页"插件"页签（启停 / 配置 / 日志 / 开发模式热重载）。

## 5. AI 集成设计（预留）

- **`AiProvider` trait 分两类**：
  - `Reranker`（本地）：重排 / 补全候选，走引擎打分管线；
  - `Generator`（云端）：整句润色 / 续写 / 翻译 / AI 指令。
- **本地路线渐进**：统计 n-gram（零依赖、立即能用）→ 量化小模型（ONNX Runtime / Candle，CPU 推理）。
- **隐私红线**：密码框（TSF 禁用 compartment / 内容类型检测）、远程桌面、用户黑名单应用 → **自动禁用一切云端调用**；云端默认关闭、显式开启、本地优先。
- **性能要求**：AI 请求全程异步，绝不阻塞按键路径；AI 候选"迟到即插入"。

## 6. 主要风险与备选

| 风险 | 应对 |
|---|---|
| TSF 文档差、坑多 | 以 imekit / khiin-rs / keymagic-3 源码为参照；阶段 0 先趟平 |
| DLL panic = 宿主应用崩溃 | 所有 COM 入口 `catch_unwind`；IPC 边界防御式解析 |
| 候选窗 WebView 延迟/内存 | 预创建预热；不达标则切 Skia/Direct2D 原生渲染（窗口抽象已留） |
| 自研引擎工作量大 | 阶段 1 范围刻意收窄（全拼+词频）；若进度告急，可临时 FFI librime 顶 MVP，接口不动 |
| 词库版权 | 只用 MIT/CC 等许可清晰数据源，`data/dict-src/` 逐条记录许可 |
| 杀软误报/签名成本 | 尽早购 EV 证书；安装包签名 + 用户指引 |

## 7. 参考资料

- [Tauri 2.12 发布公告](https://v2.tauri.app/blog/tauri-2.12/)
- [imekit —— Rust 跨平台 IME 库（含 TSF 支持）](https://github.com/SergioRibera/imekit)
- [keymagic-3 —— Rust TSF 输入法（x64/ARM64）](https://github.com/thantthet/keymagic-3)
- [rtry —— Rust TSF 输入法示例](https://github.com/quek/rtry)
- [Tauri 无焦点浮窗讨论](https://github.com/orgs/tauri-apps/discussions/14084)
