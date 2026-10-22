# 架构说明

## 一、总体分层

```
┌─────────────────────────────────────────────────────────────┐
│ frontend/  (WebView, 无构建工具, 全局脚本按序加载)              │
│                                                             │
│   views/       纯渲染：输入 state → 输出 HTML 字符串            │
│      ↑                                                      │
│   ui/          交互：事件 → 调用 IPC → 更新 state → 触发重绘     │
│      ↑                                                      │
│   core/        基础设施：state / dom / router / logs / ipc     │
└──────────────────────────┬──────────────────────────────────┘
                           │  Tauri IPC（invoke / emit）
┌──────────────────────────┴──────────────────────────────────┐
│ src-tauri/  (Rust)                                          │
│                                                             │
│   commands/    命令层：参数校验 + 转发，不写业务               │
│      ↑                                                      │
│   services/    业务层：进程拉起 / watcher / 日志 / 插件文件     │
│      ↑                                                      │
│   models/      数据结构（serde 序列化对象）                   │
│   state.rs     全局状态（AppState / WatcherCtrl）             │
│   utils/       无状态工具（路径、脱敏、分级）                  │
└─────────────────────────────────────────────────────────────┘
```

## 二、前端分层

| 目录 | 职责 | 允许依赖 |
| --- | --- | --- |
| `scripts/core/state.js` | 全局 state、常量清单 | 无 |
| `scripts/core/dom.js` | `$` / `escapeHtml` / `toast` | 无 |
| `scripts/core/ipc.js` | Tauri `invoke` 封装、配置落盘 | state / dom |
| `scripts/core/logs.js` | 日志缓冲、过滤、渲染 | state / dom |
| `scripts/core/router.js` | hash 路由、触发整页重绘 | state / views |
| `scripts/views/views.js` | 7 个视图的 HTML 模板 | state / dom |
| `scripts/ui/actions.js` | `data-action` → 处理函数映射表 | 全部（最上层） |
| `scripts/ui/interactions.js` | 账号 / 网络 / 主题 / 备注等交互 | api / state / views |
| `scripts/main.js` | 启动引导、事件订阅、导航绑定 | 全部 |

**规则**

1. 视图层只返回字符串，**不直接 `document.querySelector` 改 DOM**；重绘统一由 `render()` 触发。
2. 所有后端调用必须走 `core/ipc.js` 的 `callCmd`，不允许在视图里裸写 `invoke`。
3. `scripts/` 下按 `<script>` 顺序加载（`index.html` 末尾），**新增文件要同时改 `index.html` 的加载顺序**，并保证依赖在被依赖者之后。
4. 目前不使用 ES modules / 打包器：前端是「零构建静态资源」，改完刷新即可，构建速度只取决于 Rust 侧。

## 三、后端分层

| 目录 | 职责 | 典型文件 |
| --- | --- | --- |
| `models/` | 与前端 JSON 对齐的数据结构 | `config.rs` `plugin.rs` `status.rs` `launch.rs` `ai.rs` |
| `state.rs` | 全局可变状态，`tauri::manage` 持有 | `AppState` `WatcherCtrl` |
| `services/` | 业务逻辑，可被命令层与 watcher 复用 | `launcher.rs` `watcher.rs` `logging.rs` `detect.rs` `plugin_fs.rs` `ai_service.rs` `ai_gateway.rs` |
| `commands/` | `#[tauri::command]`，只做校验 + 调 service | `napcat.rs` `bot.rs` `plugin.rs` `app.rs` `detect.rs` `window.rs` `system.rs` `ai.rs` |
| `utils/` | 无状态纯函数 | `paths.rs` `text.rs` |

**规则**

1. `commands/` 里不写循环、不写进程管理，只做「参数校验 → 调 service → 转 `Result`」。
2. 子进程生命周期（`launch_child` / `spawn_napcat_process` / watcher）只在 `services/` 里。
3. 跨模块引用统一走 `crate::models::*` / `crate::services::*` 这类聚合再导出（各 `mod.rs` 里 `pub use`）。
4. 模块级 `#![allow(unused_imports)]` 是拆分初期的过渡措施，后续可按文件清理。

## 四、关键数据流

**启动 NapCat**

```
点击「启动」
  → ui/actions.js: start → ipc.callCmd('start_napcat')
  → commands/napcat.rs: start_napcat（校验 qq、读 config、resolve_launch）
  → services/launcher.rs: spawn_napcat_process
      → LaunchKind::Bootmain → 起 node.exe
      → LaunchKind::Script  → launch_child（cmd / pwsh / CreateProcess）
  → services/watcher.rs: spawn_watcher（等待退出码，决定是否退避重启）
  → emit napcat://status / napcat://log
  → frontend: applyStatus() / pushLog() → render()
```

**日志链路**

```
子进程 stdout → services/logging.rs: attach_log_forwarders
  → utils/text.rs: redact（token 脱敏）+ classify_log（分级）
  → emit napcat://log
  → core/logs.js: pushLog → renderLogs
  → WARN/ERROR 同时落盘 %APPDATA%\com.napcat.desktop\logs\error.log
```

## 五、新增代码往哪放

| 需求 | 落点 |
| --- | --- |
| 加一个前端页面 | `views/views.js` 加模板 + `router.js` 的 `PAGE_NAMES` + `index.html` 导航项 |
| 加一个按钮 | `views` 模板里写 `data-action="xxx"`，`ui/actions.js` 加映射 |
| 调后端新接口 | `core/ipc.js` 加薄封装 → `ui/actions.js` 调用 |
| 加一个 Tauri 命令 | `commands/` 下对应文件加 `#[tauri::command]`，记得在 `main.rs` 的 `generate_handler!` 注册 |
| 加业务逻辑 | `services/`，命令层只转发 |
| 改数据结构 | `models/`，并同步 `docs/COMMAND_API.md` 与前端 `state.js` |

## 六、兼容性约束（不可破坏）

- 20 个 command 的名字与签名是对前端的契约，见 `docs/COMMAND_API.md`。
- 3 个事件名与 payload 键固定：`napcat://log`、`napcat://status`、`napcat://qrcode`。
- `AppConfig` 字段名与磁盘 JSON 一一对应，删字段会静默丢配置。

## 七、本次重构记录

重构前：

```
napcat-app/
├─ dist/index.html      2155 行（HTML + 543 行 CSS + 1498 行 JS 全在一个文件）
├─ src-tauri/src/main.rs 2129 行（20 个命令 + 全部业务逻辑）
└─ 根目录散落 README / COMMAND_API / SECURITY_AUDIT / 截图
```

重构后：前端 5 CSS + 9 JS + 1 HTML，后端 1 入口 + 4 聚合 + 19 模块文件，文档归 `docs/`，构建脚本归 `scripts/`。
行为等价：拆分后 `cargo check` 0 error 0 warning，界面渲染与重构前一致。

## 八、防退化机制（第二轮优化的产物）

分层只解决「文件放哪」，不解决「代码会不会重新烂掉」。第二轮针对四个真正的腐化源做了收口：

| 腐化源 | 收口方式 | 约束 |
| --- | --- | --- |
| 35 处 `lock().unwrap()`，一次 panic 引发全应用连锁 panic | `utils/lock.rs::lock()`，中毒时恢复 | **禁止**再写 `.lock().unwrap()`，一律 `lock(&mutex)` |
| 15 处手拼日志事件 + 事件名字面量散落 | `services/events.rs`：`EVT_*` 常量 + `emit_log` / `emit_qrcode` | **禁止**再手写 `json!({"line":..., "level":...})` |
| 3 处裸 `unwrap()`（wait / parent / 窗口图标） | 改成带上下文的 `match` / `ok_or` | 新增代码里的 `unwrap` 必须在 review 时给出"为什么不可能失败" |
| 24 个文件挂着 `#![allow(unused_imports)]`，掩盖真实依赖 | 全部删除，按 cargo 警告精确清理 | **不允许**再用 allow 掩盖导入 |

顺带修掉一个真实隐患：`BufReader::lines().flatten()` 在持续 IO 错误时会空转（clippy 报 `flatten` 可能永不结束），已改为 `map_while(Result::ok)`。

**门禁**：`scripts/lint.ps1` 执行 `cargo clippy --all-targets -- -D warnings` + `cargo test`，任何一个警告都算失败。当前状态：clippy 零警告、7 个脱敏单测全过。

**新增代码前的三个自检**

1. 这个文件属于哪层？models / services / commands / utils，四选一，选不出说明职责没想清。
2. 进程、线程、子进程相关逻辑，`services/` 里是否已有同类实现？先复用再新增。
3. 动了 command 签名或事件 payload，`docs/COMMAND_API.md` 和 `frontend/` 是否同步？

