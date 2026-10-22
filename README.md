# NapCat Desktop

Windows 原生桌面面板，用来启动、监控和配置 NapCat QQ 机器人协议端。
纯本地运行：不联网、不上传数据、不需要账号。基于 Tauri 2（Rust + WebView2）。

## 功能

| 页面 | 用途 |
| --- | --- |
| 概览 | 运行状态、启动/停止/重启、最近日志 |
| 账号管理 | 多 QQ 号、启用开关、登录态标记 |
| OneBot 网络 | HTTP / WS / 反向 WS 服务配置 |
| 插件 | 插件目录浏览、拖放安装、可选插件一键装 |
| **AI 接入** | **多厂商 API、人设、记忆、代码块渲染的对话测试** |
| 实时日志 | 分级着色、过滤、暂停、导出 |
| 内嵌 WebUI | 直接打开 NapCat 自带 WebUI |
| 设置 | NapCat / QQ 路径、开机自启、自动重启策略 |

### AI 接入

- **多厂商**：内置 DeepSeek / OpenAI / Moonshot / 通义 / 智谱 / Ollama / Claude / Gemini 预设，也可自定义
- **三种协议族**：`openai`（兼容绝大多数厂商）、`anthropic`、`gemini`
- **人设**：一段系统提示词，可建多套随时切换
- **记忆**：按人设归属，每次对话自动拼进系统提示；可手动增删、一键清空
- **代码块**：AI 回复里的 ` ``` ` 代码块会渲染成等宽代码样式，行内代码与粗体同样支持
- **落盘**：配置 / 人设 / 记忆分别存到 `%APPDATA%\com.napcat.desktop\ai\{config,personas,memories}.json`
- **密钥安全**：请求走 `curl -K -`，含 `Authorization` 的配置从 stdin 传入，不出现在进程命令行

> 工具调用（让 AI 直接发消息、查群）目前只做到「把工具清单写进提示词」，后端不执行。
> 数据结构里的 `tool_calls` 已预留，后续接执行器时前端不用改。

### AI 自动回复（网关）

一个独立的 Node 进程，由桌面端托管，连 NapCat 的 WS 自动回消息：

- **触发**：私聊回复、群聊仅在被 @ 时回复（可分别关闭）
- **上下文**：按会话存本地 `ai/contexts/<private|group>_<id>.json`，保留最近 N 轮，重启不丢
- **记忆**：每次回复都会把「人设 + 长期记忆」拼进系统提示
- **表情包**：收到的图片自动下载到 `ai/stickers/` 并登记索引；AI 在回复里写 `[sticker:名字]` 就会把该表情发出去
- **防刷屏**：同会话冷却时间、回复最大字符数可配
- **启动即跑**：勾选「桌面端启动时自动运行」后，桌面端 setup 阶段自动拉起
- **崩溃隔离**：网关是独立进程，挂了不影响面板；它自己内置 WS 断线重连

> 所有数据都在本机 `%APPDATA%\com.napcat.desktop\ai\`，不上传任何地方。

## 目录结构

```
napcat-app/
├─ frontend/                 # 前端（Tauri WebView 加载的静态资源）
│  ├─ index.html             # 唯一页面，hash 路由切换 8 个视图
│  ├─ styles/                # 按层叠顺序拆分的样式
│  │  ├─ tokens.css          # 设计令牌 + 深浅主题变量
│  │  ├─ base.css            # 应用骨架
│  │  ├─ components.css      # 按钮 / 卡片 / Toast
│  │  ├─ views.css           # 各视图样式
│  │  ├─ ai.css              # AI 页（对话气泡 / 代码块）
│  │  └─ layout.css          # 响应式断点 + 自定义标题栏
│  ├─ scripts/
│  │  ├─ core/               # 基础设施：state / dom / router / logs / ipc
│  │  ├─ ui/                 # 交互层：按钮动作表、业务交互、AI 交互
│  │  ├─ views/              # 视图层：纯渲染函数（views.js + ai.js）
│  │  └─ main.js             # 启动引导
│  ├─ logo.png
│  └─ README.md              # 前端开发约定
├─ src-tauri/                # 后端（Rust）
│  ├─ src/
│  │  ├─ main.rs             # 入口：窗口 / 托盘 / 命令注册
│  │  ├─ state.rs            # 全局运行状态
│  │  ├─ models/             # 数据结构
│  │  ├─ services/           # 业务逻辑（进程 / 日志 / 探测 / 插件文件）
│  │  ├─ commands/           # Tauri 命令层（薄封装）
│  │  └─ utils/              # 路径与文本工具
│  ├─ Cargo.toml
│  └─ tauri.conf.json
├─ docs/                     # 文档
│  ├─ ARCHITECTURE.md        # 分层设计说明
│  ├─ COMMAND_API.md         # 前后端接口契约
│  ├─ USER_GUIDE.md          # 用户手册
│  └─ SECURITY_AUDIT.md      # 安全审计报告
├─ scripts/                  # 构建与检查脚本
│  ├─ check.ps1              # 快速编译检查
│  ├─ lint.ps1               # 严格检查（clippy 警告即失败 + 单测）
│  ├─ dev.ps1                # 开发运行
│  └─ build.ps1              # 发布构建
└─ NapCatDesktop.exe         # 发布产物（scripts/build.ps1 生成）
```

## 架构一句话

前端「视图只渲染、交互只改状态、IPC 统一收口」，后端「命令层只做参数校验与转发、业务逻辑全在 services、状态集中在 AppState」。
详见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 构建与运行

前置：Rust 工具链（`stable-x86_64-pc-windows-msvc`）+ WebView2 Runtime。

```powershell
# 编译检查
powershell -ExecutionPolicy Bypass -File .\scripts\check.ps1

# 开发运行（debug）
powershell -ExecutionPolicy Bypass -File .\scripts\dev.ps1

# 发布构建（release，产物复制到根目录 NapCatDesktop.exe）
powershell -ExecutionPolicy Bypass -File .\scripts\build.ps1
```

> 若 `cargo` 不在 PATH，脚本会自动补上 `%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin`。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 前后端分层、依赖方向、新增代码往哪放 |
| [docs/COMMAND_API.md](docs/COMMAND_API.md) | 20 个 Tauri command / 3 个事件的冻结契约 |
| [docs/USER_GUIDE.md](docs/USER_GUIDE.md) | 面向使用者的功能说明与 FAQ |
| [docs/SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md) | 安全审计发现与修复记录 |
| [docs/GIT_GUIDE.md](docs/GIT_GUIDE.md) | 提交规范、分支策略、发布流程、常用操作 |
| [CHANGELOG.md](CHANGELOG.md) | 每个版本的变更记录 |
