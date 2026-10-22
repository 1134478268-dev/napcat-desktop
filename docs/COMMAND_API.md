# NapCat Desktop · Tauri Command 接口清单

> 本文档由 Rust 后端维护者编写，前端对接方严格按此实现。
> 所有字段名、事件名、错误字符串都已冻结，不要临时改。
> 前端调用入口：`window.__TAURI__.core.invoke(cmd, args)`
> 事件监听：`window.__TAURI__.event.listen('napcat://xxx', e => { ...e.payload ... })`
>
> **本文档已按当前实现（20 个 command / 3 个事件 / 双启动形态）全面重写**，
> 早期只列 8 个 command 的版本已作废。

---

## 数据结构

### AppConfig
磁盘配置对象，序列化到 `app_config_dir/napcat-desktop.json`。

| 字段 | 类型 | 说明 |
|---|---|---|
| `napcat_path` | string | NapCat 工作目录（含启动脚本的那一层） |
| `qq_path` | string | QQNT 安装目录（仅兼容性提示用） |
| `plugins_dir` | string | 插件目录，由后端 `resolve_plugins_dir` 从 `napcat_path` 自动推断 |
| `auto_restart` | bool | 崩溃后是否自动拉起，默认 `true` |
| `max_restarts` | number | 单次启动会话内最多连续自动重启次数，默认 `5` |
| `start_on_boot` | bool | 开机自启（后端仅持久化，不负责注册） |
| `minimize_to_tray` | bool | 关闭时最小化到托盘，默认 `true` |
| `proxy` | string | HTTP/SOCKS5 代理，留空直连 |
| `accounts` | Account[] | QQ 账号列表 |
| `onebot` | OneBotService[] | OneBot 网络服务列表 |
| `plugin_memos` | object | 插件备注：`{ "<插件目录名>": "<备注>" }` |

> 所有字段都带 `serde(default)`，旧版本配置文件可以向前兼容。

### Account
```json
{ "qq": "10001", "nick": "主号", "enabled": true, "saved_login": true }
```
- `enabled`：只影响「`start_napcat` 不带 `qq` 参数时挑哪个账号」
- `saved_login`：该账号是否成功登录过（由前端在收到 `status.running=true` 时置位）

### OneBotService
```json
{ "type": "HTTP", "port": 3000, "addr": "0.0.0.0", "token": "", "memo": "", "enabled": true }
```
- `type`：Rust 侧字段名是 `tp`，通过 `#[serde(rename = "type")]` 映射，**前端只使用 `type`**
- `type` 取值约定：`HTTP` / `WS` / `反向WS`（后端不校验，原样透传）
- `token`：鉴权 token（可选）；**必须保留该字段**，否则 serde 会静默丢弃前端传来的值

### PluginInfo
`list_plugins` 返回数组元素。
```json
{
  "dir": "anti-recall",
  "name": "防撤回助手",
  "version": "0.5.1",
  "desc": "...",
  "author": "社区",
  "enabled": true,
  "has_exec": true
}
```
- `dir`：目录名（已去掉 `.disabled` 后缀）
- `enabled`：目录名是否**没有** `.disabled` 后缀
  （注意：这是本面板自己的识别规则，不代表 NapCat 会因此停止加载插件）
- `has_exec`：是否存在 `index.js` / `main.js` / `main.py`

### StatusInfo
既是 `get_napcat_status` 的返回值，也是 `napcat://status` 事件的 payload。
```json
{
  "running": true,
  "pid": 12345,
  "last_exit_code": null,
  "restart_count": 0,
  "message": "已启动，QQ=10001"
}
```
- `pid`：未运行时为 `null`
- `last_exit_code`：进程退出后为对应退出码；未退出过为 `null`
- `restart_count`：本次启动会话内已被自动拉起过几次
- 事件版本额外携带 `qq` 字段（见下）

---

## Commands

### 1. `ping() -> string`
联通性测试，返回 `"pong from napcat-desktop"`。无副作用。
> 目前前端未使用，保留作为联调探针。

### 2. `load_config() -> AppConfig`
读取磁盘配置。文件不存在时返回全空 `AppConfig`（**不报错**）。
成功后同步到后端内存缓存，并会**自动修正 `plugins_dir`**：若配置里的值不存在，用 `napcat_path` 重新推断。

**错误字符串**：
- `"无法获取 app_config_dir: ..."`
- `"创建配置目录失败: ..."`
- `"读取配置失败: ..."`
- `"配置 JSON 损坏: ..."`

```js
const cfg = await window.__TAURI__.core.invoke('load_config');
```

### 3. `save_config(config: AppConfig) -> null`
整对象覆盖写回 `napcat-desktop.json`，并更新内存缓存。
**不是**增量 patch——前端必须先 `load_config` → 改字段 → 整个回传。

**错误字符串**：`"序列化失败: ..."` / `"写入配置失败: ..."`

```js
await window.__TAURI__.core.invoke('save_config', { config: newCfg });
```

### 4. `detect_napcat_path() -> string[]`
扫描固定安装根目录 + `D:\` 一级子目录（名字含 `napcat`/`nap`），返回候选绝对路径。找不到返回 `[]`。

**不扫描**桌面、下载、用户主目录——防止执行伪造的 `launcher.bat`。

判定标志（任一命中即视为 NapCat 目录）：
`launcher-user.bat` / `launcher.bat` / `start-napcat.bat` / `start-napcat.ps1` /
`napcat.ps1` / `napcat.exe` / `bootmain\NapCatWinBootMain.exe` /
`shell\napcat.bat` / `shell\launcher.bat` / `config\` 目录

### 5. `detect_qq_path() -> string[]`
检测 QQNT 安装目录，返回候选列表。顺序：
1. 注册表 `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\QQ` 的 `UninstallString` 反推
2. 注册表 `HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT` 的 `Install` 值
3. 常见安装位（Program Files / D 盘 Tencent 等）

判定：目录下存在 `QQ.exe`。

### 6. `pick_directory(defaultPath?: string) -> string`
弹系统「浏览文件夹」对话框（PowerShell + FolderBrowserDialog），返回用户选的目录；**取消返回空串**。

**错误字符串**：`"无法启动目录选择对话框: ..."`

### 7. `list_plugins(path: string) -> PluginInfo[]`
扫描给定插件目录的**一级子目录**。
- 跳过以 `.` 开头的隐藏目录
- 目录名以 `.disabled` 结尾 → `enabled=false`，`dir` 字段去掉后缀
- 读 `<dir>/package.json` 抽 name/version/description/author（author 兼容字符串与 `{name:...}` 两种写法）
- 读不到就用目录名兜底；按 `name` 排序
- `<dir>/package.json` 超过 **1 MiB**（`MAX_PACKAGE_JSON_BYTES`）时跳过解析，退回目录名

### 8. `install_plugin(source: string) -> string`
把拖入的**文件夹**或 **`.zip`** 安装到插件目录。返回一句人类可读的成功提示。

- 文件夹 → 递归复制到 `<plugins_dir>/<目录名>`
- `.zip` → PowerShell `Expand-Archive` 解压到 `<plugins_dir>/<zip 主名>`
- 目标已存在 → 报错，**不覆盖**
- 其他扩展名 → 拒绝
- 文件夹递归复制会**跳过符号链接 / junction**，并限制 **32 层**（`MAX_COPY_DEPTH`）深度，防止环形链接导致无限递归 / 栈溢出

**错误字符串**：
- `"插件目录未配置，请先在设置页设置 NapCat 路径"`（防止把插件复制到进程当前目录）
- `"创建 plugins 目录失败: ..."`
- `"源路径不存在: ..."`
- `"插件目录已存在: ..."`
- `"复制文件夹失败: ..."`
- `"不支持的文件类型 .xxx，请拖入文件夹或 .zip 文件"`
- `"无法识别的文件类型"`
- `"调用 Expand-Archive 失败: ..."` / `"解压失败: ..."`

### 9. `install_plugin_from_url(app?, url: string, name: string) -> string`
从 URL **一键安装**插件（插件页「可选插件」区的「安装」按钮用它）。返回一句成功提示。

- **异步命令**（`async fn`）：下载最长可能等 300 秒，同步命令会冻住主线程，所以它跑在 async runtime 上
- **域名白名单**：只允许 `https://` + `github.com` / `codeload.github.com` /
  `objects.githubusercontent.com` / `raw.githubusercontent.com` / `api.github.com`。
  这是本项目唯一"由前端给 URL、后端联网取文件"的通道，不限制等于把"下载任意远程文件"交给渲染进程
- **镜像兜底**：直连 `github.com` 失败时，自动依次重试 `ghproxy.net` / `gh-proxy.com` / `ghfast.top`
  三个公共镜像（实测本机直连时好时坏，镜像稳定）；成功来源会写进 `error.log`
- **代理**：`AppConfig.proxy` 非空时通过 `curl --proxy` 使用
- `name` 只允许字母数字与 `.` `_` `-`（防路径穿越），落盘为 `plugins_dir/<name>`
- 解压优先用系统自带 `tar.exe`（不依赖 PowerShell 模块），失败才回退 `Expand-Archive`
- 解压结果若只套一层目录会自动展平（GitHub 的 zip 常见结构）
- 过程与失败原因都写入 `error.log`（`[plugin-install]` 前缀），便于事后排查

**错误字符串**：`"只允许 https:// 开头的下载地址"` / `"下载地址的域名不在白名单内: ..."` /
`"插件名不合法: ..."` / `"插件目录已存在: ..."` / `"下载失败：...；已尝试直连与 N 个公共镜像。..."` /
`"解压失败：tar: ...；Expand-Archive: ..."` / `"移动解压结果失败: ..."`

### 10. `start_bot() -> null`
在插件目录的一级子目录里找**第一个** `bot.py`，以它所在目录为 cwd 执行 `python bot.py`，日志加 `[bot]` 前缀推给前端。
**不会自动重启**；退出后清空状态并推一条 warn 日志。

**错误字符串**：
- `"机器人已在运行"`
- `"在 ... 下未找到 bot.py（NoneBot 入口）"`
- `"插件目录未配置，请先在设置页设置 NapCat 路径"`
- `"启动 python bot.py 失败: ..."`

### 10. `stop_bot() -> null`
`taskkill /F /T` 结束后台线程，等 400 ms 后清空 pid。未运行是 no-op。

### 11. `get_bot_status() -> bool`
机器人进程是否在运行。

### 12. `start_napcat(qq?: string) -> StatusInfo`
拉起 NapCat 子进程。这是最核心的命令。

**参数校验（安全审计 H1，硬性）**：
- `qq` 非空时必须是 **5–11 位纯数字**，否则返回 `"QQ号必须是 5-11 位纯数字"`
- 不传 `qq` 时后端自动选 `config.accounts` 里第一个 `enabled` 的账号
- `config.napcat_path` 必须非空

**启动形态（自动判定，见下方专节）**：
- 整合包形态：`shell\node.exe` + `shell\index.js` 同时存在 → 直接起 node
- 脚本形态：按优先级挑启动器，交给 `launch_child`

**launcher 安全校验**（在 `launch_child` 内）：
`canonicalize` → 存在性 → 扩展名白名单（`.bat` / `.ps1` / `.exe`）→
拒绝含 `& | < > ^` 的路径 → 要求 launcher 位于 `napcat_path` 目录树内

**副作用**：
- spawn 子进程（`CREATE_NO_WINDOW`，不弹黑窗）
- 起 stdout/stderr 转发线程，开始 emit `napcat://log`（token 已脱敏）
- 识别二维码并 emit `napcat://qrcode`
- 起 watcher 线程，子进程退出后按指数退避自动拉起
- emit 一次 `napcat://status`（`running=true`，带 `qq`）

**错误字符串**：
- `"NapCat 已在运行"`
- `"QQ号必须是 5-11 位纯数字"`
- `"NapCat 路径未配置，请先在设置页选择或自动检测"`
- `"在 <napcat_path> 下未找到启动脚本；可接受的形式：shell\node.exe + shell\index.js（bootmain 整合包）、launcher-user.bat、launcher.bat、start-napcat.bat、napcat.exe、start-napcat.ps1、shell\napcat.bat"`
- `"launcher 路径不存在或无法访问: ..."`
- `"launcher 扩展名必须是 .bat / .ps1 / .exe，实际是 .xxx"`
- `"launcher 路径包含非法 shell 元字符"`
- `"launcher 不在 NapCat 目录内"`
- `"启动 node.exe 失败: ..."` / `"spawn 失败: ..."`

```js
await invoke('start_napcat', { qq: '10001' });
```

### 13. `stop_napcat() -> null`
主动停止。先把 watcher 的 `auto_restart` 关掉，再 `taskkill /F /T` 整棵进程树；
后台线程等 500 ms 后用 `tasklist` 复核进程是否真的消失，确认后清空运行状态。
**不返回错误**，未运行也是 no-op（只推一条 warn 日志）。

### 14. `kill_napcat() -> null`
模拟崩溃：**先强制**把 `auto_restart` 置 `true`（保证 watcher 会复活），再 `taskkill /F /T`。
watcher 看到非零退出码后按指数退避自动拉起——用于演示崩溃恢复。

**错误字符串**：`"NapCat 未在运行"`

### 15. `get_napcat_status() -> StatusInfo`
同步查询当前状态。运行状态取自独立的权威标志（**不是**子进程句柄槽），
因此即使 watcher 正持有句柄等待，也能返回正确的 `running` / `pid`。

### 16. `win_minimize() -> null`
最小化主窗口。**错误字符串**：`"找不到主窗口"`

### 17. `win_toggle_max() -> null`
最大化 / 还原主窗口。**错误字符串**：`"找不到主窗口"`

### 18. `win_close() -> null`
请求关闭主窗口。注意：Rust 侧拦截了 `CloseRequested`，所以实际行为是**隐藏到托盘**，
程序不会退出（唯一退出入口是托盘菜单的「退出」）。

### 19. `open_external(path: string) -> null`
用系统默认程序打开：
- `http://` / `https://` 开头 → 默认浏览器
- 其他 → 必须是磁盘上存在的路径，否则报错
- 拒绝含 `& | < > ^` 等 shell 元字符的输入（防止 `cmd /C start` 被注入）

**错误字符串**：`"路径为空"` / `"路径包含 shell 元字符或引号，已拒绝打开"` / `"路径不存在: ..."` / `"打开 URL 失败: ..."` / `"打开路径失败: ..."`
> 拒绝的字符集：`& | < > ^ " % ! \`` 与换行。其中 `"` 是独立审计实测出的注入关键字符
> （Rust 把 `"` 转义成 `\"`，而 cmd.exe 只认裸 `"`）；`%`/`!` 分别对应 cmd 的变量展开
> 与延迟展开；换行同样是 cmd 的语句分隔符。

### 20. `export_logs(content: string) -> string`
把前端传来的日志文本写入 `app_config_dir/logs-<时间戳>.txt`，返回**绝对路径**。
文件名时间戳取**本地时区**。为防一次 IPC 写满磁盘，超过 8 MiB 直接报错。

**错误字符串**：`"导出内容过大（N 字节，上限 8388608 字节），请先清空或分批导出"` / `"无法获取配置目录: ..."` / `"创建目录失败: ..."` / `"写入失败: ..."`

---

## Events（后端 → 前端）

### `napcat://status`
子进程状态变化时推送。payload 是 `StatusInfo` **加上 `qq` 字段**：
```json
{
  "running": false,
  "pid": 12345,
  "qq": "10001",
  "last_exit_code": 1,
  "restart_count": 2,
  "message": "进程异常退出 code=1，4s 后自动重启"
}
```
- `qq`：本次启动使用的 QQ 号；前端据此判断"哪个账号在运行"。未运行时可能为空/缺失。
- 触发时机：启动成功 / 崩溃待重启 / 重启成功 / 正常停止 / 放弃重启 / watcher 发现无人持有句柄

**重要**：`message` 是给用户看的自然语言，前端**不要**用退出码单独推断崩溃，
应优先按 `message` 语义判断（含「正常退出」「主动停止」「未运行」→ 非崩溃；
含「异常退出」「自动重启」「崩溃」「异常」→ 崩溃），退出码只做兜底。

### `napcat://log`
子进程 stdout/stderr 一行一条，另外也用于后端的内部提示日志。
```json
{ "line": "[NapCat] 账号 10001 登录成功", "level": "ok", "source": "napcat" }
```
`level` 取值：`info` / `ok` / `warn` / `err` / `recv` / `plugin`
`source` 取值：`napcat` / `ai` / `bot`（v0.4 新增；旧前端忽略该字段仍可工作）
（前端应做**白名单**处理，未知级别一律按 `info` 渲染，未知来源一律按 `napcat`。）

> **落盘分流**：`source=ai` 的 warn/err 写 `logs/ai-error.log`，
> 其余写 `logs/error.log`。排查时按文件看，不必在前缀里肉眼过滤。

> **脱敏**：后端在 emit 前会把 `token=xxx` / `access_token=xxx` / `authorization: xxx`
> 替换成 `***`，前端不需要再做一次。

### `napcat://qrcode`
后端从子进程日志里识别到二维码图片路径后，读文件转 base64 推送：
```json
{ "data_url": "data:image/png;base64,iVBORw0KG..." }
```
前端弹窗展示即可；扫码成功/失败可由 `napcat://log` 的文案进一步判断并自动关窗。

**JS 订阅示例**：
```js
const { listen } = window.__TAURI__.event;
await listen('napcat://status', e => { /* e.payload 更新状态徽章 */ });
await listen('napcat://log',    e => { const { line, level } = e.payload; });
await listen('napcat://qrcode', e => { img.src = e.payload.data_url; });
```

---

## 启动形态（backend 内部判定）

| 形态 | 判定条件 | 启动方式 | 工作目录 | 自动重启 |
|---|---|---|---|---|
| **整合包 / bootmain** | `shell\node.exe` 与 `shell\index.js` 同时存在 | `node.exe index.js [-q QQ]`，并注入 `quick-login.env` 里的环境变量 | `shell\` | **禁用** |
| **脚本** | 否则按优先级挑启动器 | 见下 | `napcat_path` | 启用 |

脚本形态的启动器优先级：
`launcher-user.bat` → `launcher.bat` → `start-napcat.bat` → `napcat.exe` → `start-napcat.ps1` → `shell\napcat.bat`

- `.bat` 经 `cmd /C` 执行；`.ps1` 经 `pwsh -NoProfile -ExecutionPolicy Bypass -File` 执行；`.exe` 直接 `CreateProcess`（不经 shell，攻击面最小）
- 所有形态都用 `CREATE_NO_WINDOW`，不弹控制台窗口
- **已知限制**：仅含 `bootmain\NapCatWinBootMain.exe` 的目录能被 `detect_napcat_path` 识别，但**无法自动启动**（其调用约定未确认），会报"未找到启动脚本"

---

## 崩溃自动拉起策略（watcher 行为）

| 退出码 | auto_restart | restart_count < max | 行为 |
|---|---|---|---|
| 0 | 任意 | 任意 | emit stopped（"进程正常退出"），退出 watcher |
| ≠0 | false | 任意 | emit stopped（"已通过桌面端主动停止"），退出 watcher |
| ≠0 | true | < max | 等 `1 << min(restart_count, 4)` 秒（1/2/4/8/16s）后重启，`restart_count+1` |
| ≠0 | true | ≥ max | emit "连续崩溃 N 次，已放弃自动重启"，退出 watcher |
| 任意 | 任意 | — | 整合包（bootmain）形态下**永不自动重启** |

`max_restarts` 默认 5，可在 `AppConfig.max_restarts` 改（设置页提供 3 / 5 / 10）。
退出 watcher 时后端会把运行状态与 pid 一并清空，避免残留"运行中"的假状态。

---

## 配置文件与日志位置

| 内容 | 路径 |
|---|---|
| 配置 | `%APPDATA%\com.napcat.desktop\napcat-desktop.json` |
| 错误日志 | `%APPDATA%\com.napcat.desktop\logs\error.log`（只记 WARN/ERROR） |
| 导出日志 | `%APPDATA%\com.napcat.desktop\logs-<YYYYMMDD-HHMMSS>.txt` |

均由 Tauri 的 `app_config_dir()` 决定，前端不需要关心绝对路径。

---

## 变更记录

- 补齐 `OneBotService.token` 字段（此前前端发送的值会被 serde 静默丢弃）
- 运行状态改用独立权威标志，修复"watcher 持有句柄期间 `running` 恒为 false"导致的重复启动
- 统一首启与自动重启的启动路径（`LaunchKind` / `resolve_launch` / `spawn_napcat_process`）
- 修复日志脱敏函数 `redact` 的无限循环
- 新增安全校验：launcher 必须位于 `napcat_path` 目录树内；`install_plugin` 拒绝空插件目录；`open_external` 拒绝 shell 元字符；`export_logs` 限制 8 MiB
- 导出日志文件名改用本地时区
- 删除未使用的 `bot_child` 字段
- 修复 `redact` 对 `Authorization: Bearer <token>` 的脱敏不完整：原实现只把 scheme 打码，
  真正的 token 本体仍会原样输出（例如 `Authorization: *** xyz`），现改为连 scheme 一起跳过再打码
- 修复 `kill_napcat` 的 E0597 借用错误（`MutexGuard` 临时值生命周期跨越了 `st` 的析构点）
- watcher 在退避等待期间**复查** `auto_restart`：原实现在这段窗口内点「停止」仍会被自动拉起
- `resolve_launch` 候选补齐 `start-napcat.ps1` 与 `shell\napcat.bat`，与 `is_napcat_dir` 的识别范围对齐
- 读取 `package.json`（1 MiB）与目录递归复制（32 层 + 跳过符号链接）加资源上限
- 前端：拆分 `enabled`（持久配置）与运行态（`runningQq` 瞬时事实），修复保存配置会把其他账号
  写成 `enabled:false` 的污染；`saved_login` 改为仅由日志中的"登录成功"点亮；插件开关改为只读
  状态标签（原开关不持久化，属虚假承诺）

---

## AI 接入（v0.2 新增，5 个命令）

配置文件：`%APPDATA%\com.napcat.desktop\ai\{config,personas,memories}.json`。
整体读、整体写，不做增量 patch（与 `AppConfig` 同一套思路）。

### 21. `ai_load_state() -> AiState`
```json
{
  "config": { "active_provider": "p-xxx", "providers": [...], "active_persona": "persona-xxx",
              "system_extra": "", "temperature": 0.7, "max_tokens": 2048,
              "memory_enabled": true, "memory_limit": 50, "tool_calling_enabled": false },
  "personas": [{ "id": "persona-xxx", "name": "群助手", "prompt": "...", "created_at": "..." }],
  "memories": [{ "id": "mem-xxx", "persona_id": "", "content": "...", "created_at": "...", "source": "manual" }]
}
```
文件不存在时返回全默认值，不报错。

### 22. `ai_save_config(config: AiConfig) -> null`
整对象覆盖写 `ai/config.json`。

### 23. `ai_save_personas(personas: Persona[]) -> null`
### 24. `ai_save_memories(memories: MemoryEntry[]) -> null`

### 25. `ai_chat(providerId: string, messages: ChatMessage[]) -> AiChatResult`

`ChatMessage { role: "user"|"assistant"|"system", content: string }`

后端按 provider 的 `kind`（`openai` / `anthropic` / `gemini`）组装请求，并自动拼 system prompt：
当前人设 → `system_extra` → 记忆（若 `memory_enabled`）→ 工具清单（若 `tool_calling_enabled`）。

返回值：
```json
{ "content": "...", "model": "...", "prompt_tokens": 12, "completion_tokens": 34, "tool_calls": [] }
```

`tool_calls` 是 **预留字段**：当前恒为空，后端不执行任何工具。

**错误字符串**：`"尚未配置任何 AI 服务，请先在 AI 页添加"` / `"「X」的 API 地址为空"` /
`"「X」的模型名为空"` / `"「X」的 API Key 为空"` / `"无法启动 curl（系统是否自带 curl.exe？）: ..."` /
`"请求失败（curl 退出码 N）: ..."` / `"上游返回 HTTP N: ..."` / `"上游没有返回内容: ..."`。

> 密钥安全：请求通过 `curl -K -` 执行，包含 `Authorization` 的完整配置从 **stdin** 传入，
> 不出现在进程命令行；请求体写临时文件，调用结束立即删除。

---

## AI 自动回复网关（v0.3 新增，3 个命令）

网关是一个独立的 Node 进程（`assets/ai-gateway.mjs`，编译期内嵌进 exe），
负责连 NapCat 的 WS 收消息、调 AI、回消息。桌面端只负责启停与日志转发。

### 26. `ai_gateway_start() -> AiGatewayStatus`
校验「自动回复已启用 + 当前 provider 有 key/model/base」，然后 spawn `node gateway.mjs`。

**错误字符串**：`"AI 网关已在运行"` / `"自动回复未启用：请先在 AI 页打开「启用自动回复」"` /
`"尚未配置 AI 服务"` / `"「X」的 API Key / 模型 / 地址不完整"` / `"启动 node 失败（需要 Node.js 在 PATH 中）: ..."`

### 27. `ai_gateway_stop() -> null`
`taskkill /F /T` 结束整棵进程树。未运行时返回 `"AI 网关未在运行"`。

### 28. `ai_gateway_status() -> AiGatewayStatus`
```json
{ "running": true, "pid": 12345, "message": "" }
```

**网关行为约定**

| 项 | 说明 |
| --- | --- |
| 连接 | 连 `gateway.ws_url`，断线 3 秒自动重连 |
| 触发 | 私聊（`reply_private`）/ 群聊被 @（`reply_group_at`） |
| 上下文 | 每个会话一个文件：`ai/contexts/<private|group>_<id>.json`，保留 `context_rounds` 轮 |
| 冷却 | 同会话 `cooldown_ms` 内不重复回复 |
| 表情保存 | 收到 image 段时下载到 `ai/stickers/`，登记到 `index.json` |
| 表情发送 | 回复里 `[sticker:名字]` 会转成 image 段发出 |
| 自动启动 | `gateway.auto_start && gateway.enabled` 时，桌面端 setup 阶段拉起 |

`AiGateway` 配置字段（存于 `ai/config.json` 的 `gateway`）：
`enabled, auto_start, ws_url, http_url, access_token, reply_private, reply_group_at,
context_rounds, cooldown_ms, max_reply_chars, save_stickers, send_stickers`。

> 旧版 `config.json` 没有 `gateway` 字段时会用 `Default` 补全，不会报错。
