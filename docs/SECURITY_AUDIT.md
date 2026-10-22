# NapCat Desktop · Tauri 2.0 安全审计报告

> ## ⚠️ 修订状态（后续更新，请先读这里）
>
> 本报告基于**早期只读快照**（当时后端只有 3 个 command、前端还是 inline onclick 版本），
> 下列问题已在后续迭代中修复或部分修复。原文保留作为历史记录，**其中行号已过时**，
> 引用代码位置请以 `src-tauri/src/main.rs` 与 `dist/index.html` 为准。
>
> | 审计项 | 当前状态 | 修复方式 |
> |---|---|---|
> | H1 `cmd /C` 命令注入 | ✅ 已修 | 从不拼 shell 字符串（始终传 args 数组）；`canonicalize` + 扩展名白名单（`.bat`/`.ps1`/`.exe`）+ shell 元字符黑名单；QQ 号强制 5–11 位纯数字；**launcher 必须位于 `napcat_path` 目录树内**（防止 IPC 面被滥用为"执行任意盘任意脚本"） |
> | H2 存储型 XSS | ✅ 已修 | 全量 `escapeHtml`（含属性值）；`onclick` / `onchange` 等内联事件属性已清零（0 处），改用 `data-action` 全局事件委托 + `data-*` 传参 |
> | H3 CSP 为 null | ✅ 已修 | `tauri.conf.json` 已配置 CSP。**v0.4.1 起 `script-src` 去掉了 `'unsafe-inline'`**（前端已无内联 script / 内联事件 / `javascript:`，构建后实测界面与 IPC 正常）；`style-src` 仍保留 inline，因为模板里有大量 `style="..."` 属性 |
> | M1 启动路径无校验 | ✅ 已修 | 同 H1；`detect_napcat_path` 只扫固定安装根目录，**不扫**桌面/下载/用户主目录 |
> | M2 日志敏感信息 | ✅ 已修（v0.4.1） | `redact` 已修无限循环**与** `Authorization: Bearer <token>` 脱敏不完整。本轮补齐键表：新增 `client_secret` / `clientsecret` / `private_key` / `privatekey`；驼峰写法（`accessToken` / `apiKey`）由大小写不敏感匹配天然覆盖。新增单测 `redacts_camel_case_and_oauth_fields`，脱敏测试共 8 个。**仍不覆盖**：URL 编码形式、群消息体与 CQ 码（引入 regex 可解，刻意不引） |
> | M3 配置敏感字段 | ✅ 已修（v0.4.1） | 新增 `services/secret.rs`：用 **Windows DPAPI** 加密落盘的 `api_key` 与 OneBot `access_token`，格式 `dpapi:<hex>`。读时自动解密、写时自动加密，无前缀的旧明文原样兼容（下次保存自动升级）。**边界如实说明**：DPAPI 绑定当前用户+机器，防的是配置被拷走/同步网盘/截图，不防同机同用户的其他进程 |
> | L1 关闭最小化不可绕过 | ✅ 保持 | `CloseRequested` 统一 `hide() + prevent_close()`，唯一退出入口是托盘菜单 |
> | L2 依赖版本 | ✅ 无已知高危 | v0.4.1 起 CI 增加 `rustsec/audit-check`（本机无法联网核实 advisory，交给 CI 的官方数据库） |
> | L3 `#[cfg]` 跨平台编译问题 | ✅ 已修 | 该写法已不存在 |
>
> **后续新增的加固**（不在原报告范围内）：
> `install_plugin` 拒绝空/相对插件目录（否则会把插件复制到进程当前目录）、
> 目录递归复制跳过符号链接 / junction 并限制 32 层深度、`package.json` 读取上限 1 MiB、
> `open_external` 拒绝 shell 元字符与引号（见下）、`export_logs` 上限 8 MiB、
> `redact` 改为大小写不敏感的键名匹配并覆盖 JSON / 引号 / 口令类字段（含 7 个单元测试）。
>
> **H1 根因的第二处落点（后续审计发现）**：Rust 把参数内的 `"` 转义成 `\"`、而 cmd.exe
> 只认裸 `"` 这同一个根因，除已修的 `start_napcat` 外还在 **`open_external` 的
> `cmd /C start "" <path>`** 上存在，独立审计用 rustc 探针实测可注入（payload 形如
> `http://x/a b" & cmd /c killme.cmd & "`，触发条件是"空格 + 一个双引号"）。
> 现已通过拒绝 `& | < > ^ " % ! \`` 与换行关闭该通道；长期建议改用 `ShellExecuteW`
> 彻底摆脱 cmd 的二次解析。
>
> **反向结论（不要误改）**：独立审计实证 Zip Slip 在 PS 5.1 与 pwsh 7 上均被拒绝、
> `launch_child` 无法注入、`pick_directory` / `install_plugin` 的 PowerShell 拼接
> 无法注入、前端 67 处插值除 3 处受控项外全部安全。补充审计报告
> 未随仓库发布（含本机环境信息）。

- 审计日期：2026-10-14
- 审计对象（当前只读快照，未改动任何代码）：
  - `dist/index.html`（前端单文件，7 视图）
  - `src-tauri/src/main.rs`（Rust 后端，3 个 command）
  - `src-tauri/tauri.conf.json`
  - `src-tauri/Cargo.toml`
- 补充核对：`src-tauri/Cargo.lock` 解析到的实际版本为 **tauri 2.12.3 / tauri-runtime 2.12.1 / wry 0.57.0 / serde 1.0.229 / serde_json 1.0.154**；项目内**没有** `capabilities/` 目录（只有 `gen/schemas/` 自动生成件），即未挂任何 Tauri 插件权限面。
- 说明：另一个子代理正在并行补功能，本报告基于当前快照；行号以快照为准，修复时请以实际代码复核。

---

## 一、风险清单

### 🔴 高严重度（上线前必须修）

---

#### H1. `cmd /C` + 未校验参数 → 命令注入 / 任意进程启动

- **位置**：`src-tauri/src/main.rs:31-35`
  ```rust
  let proc = Command::new("cmd")
      .args(["/C", &launcher_path, &qq])
      .spawn()
  ```
- **问题描述**：
  1. 虽然 `Command::args()` 走的是 argv 数组（不经过 shell 拼接字符串），但这里**显式起了 `cmd.exe /C`**，等于又把整条命令行交回 cmd.exe 做一遍 shell 解析。`cmd /C` 会解释 `& | && || > < ^ ( ) %VAR%` 等元字符。
  2. Rust 在 Windows 下按 MSVCRT 规则引用参数（内部 `"` 转成 `\"`），而 **cmd.exe 不识别 `\"` 转义，它只按裸 `"` 切换引号状态**——这是 Windows 上经典的“Rust argv 引用 ↔ cmd 引号解析不一致”。于是 `qq = 123"&calc.exe&"` 这类输入可以逃出引用、追加任意命令。朴素的 `123 & calc.exe`（带空格）可能被 Rust 整体加引号挡掉，但带引号/反斜杠的 payload 实测可破。
  3. `start_napcat` 的 `launcher_path`、`qq` **完全来自前端 invoke**（main.rs:26），后端没有任何校验：qq 不校验是否纯数字、launcher_path 不校验是否存在/是否 .bat/.exe/是否在用户选定目录内。
  4. 攻击链：渲染进程里任何一段 JS（包括 H2 的 XSS 注入脚本）都能 `invoke('start_napcat',{launcher_path:'任意.bat', qq:'x"&恶意命令&"'})` → **任意命令执行**。
- **修复建议**：
  ```rust
  // 1) qq 只允许 5~11 位数字
  if !(qq.len() >= 5 && qq.len() <= 11 && qq.chars().all(|c| c.is_ascii_digit())) {
      return Err("QQ 号格式非法".into());
  }
  // 2) launcher_path：规范化 + 存在性 + 扩展名白名单
  let abs = std::fs::canonicalize(&launcher_path).map_err(|_| "launcher 路径不存在")?;
  let ext = abs.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
  if ext != "bat" && ext != "exe" {
      return Err("只允许启动 .bat/.exe".into());
  }
  // 3) 输入已证明无 shell 元字符后，再经 cmd /C 启动；或直接 Command::new(&abs).arg(&qq)
  let proc = Command::new("cmd")
      .args(["/C", &abs, &qq])
      .spawn()
      .map_err(|e| format!("启动失败: {e}"))?;
  ```
- **防护注释建议（修复后抄进代码）**：
  ```rust
  // 安全：此处必须传 args 数组而非拼接命令字符串。
  // 虽然走了 cmd /C，调用前已对 qq 做“纯数字”校验、对 launcher_path 做
  // canonicalize + 扩展名白名单，确保二者不含 & | > ^ 等 cmd 元字符，
  // 否则 Rust 的 MSVCRT 引号规则与 cmd 的引号解析不一致会被注入。
  ```

---

#### H2. 前端多处把动态数据原样塞进 innerHTML → 存储型 XSS（在 Tauri 里 = 间接 RCE）

- **位置**：
  - 账号视图 `dist/index.html:558`（`style="background:${a.color}"`）、`:562`（`${a.nick}` 裸插）、`:577`（`onclick="toggleAcct('${a.qq}')"`，qq 含单引号即属性逃逸）
  - 网络视图 `:598`（`${s.addr}` 裸插）、`:599`（`value="${s.memo||''}"`，memo 含 `"` 即属性逃逸）
  - 插件视图 `:621` `${p.name}`、`:624` `${p.dir}`、`:628` `${p.desc}`、`:630` `${p.author}` 等**全部裸插**
- **问题描述**：
  - 这些位置目前用的是写死的 demo 数据，今天不可直接利用；但一旦接真数据：QQ 备注、服务备注（用户可输入，见 `saveMemo` :954）、以及**插件目录里 package.json 的 name/desc/author**（产品逻辑是“把插件文件夹丢进 plugins/ 目录”，见 :641）都会被原样渲染成 HTML。
  - 一个恶意插件文件夹的 `package.json` 里写 `"name": "<img src=x onerror=...>"`，下次刷新插件列表就在渲染进程里执行 JS。
  - 在 Tauri 里，渲染进程脚本能调用已注册的 `start_napcat`，所以 **XSS → invoke → H1 的任意进程启动**，危害升级为 RCE。
- **做得对的地方（请保持）**：
  - 日志行已转义：`renderLogs` :778、`renderMiniLog` :783 都用了 `escapeHtml(l.msg)`——所以日志里出现 `<script>` 会当纯文本显示，不会执行。
  - `toast()` :483 用 `textContent`；`editNick` :934、`saveNick` :949 都调了 `escapeHtml`。这些是正确范例。
- **修复建议**：
  - 所有动态插值一律 `escapeHtml(...)`，包括进入属性值的地方：`value="${escapeHtml(s.memo)}"`、`onclick` 不要再内联拼字符串——改成渲染后 `addEventListener`。
  - `${a.color}` 进 `style` 是 CSS 注入面，应改成白名单选色或校验 `^#[0-9a-fA-F]{3,8}$`。
  - 中长期：弃用模板字符串拼 `innerHTML`，改用 `createElement + textContent`。
- **防护注释建议（修复后抄进代码）**：
  ```js
  // 安全：escapeHtml 必须在“这里”调用——本函数把用户/插件提供的字符串
  // 直接拼进 innerHTML。它转义 & < > " ' 五个字符，防止备注/插件名里的
  // HTML 被当成标签执行（Tauri 渲染进程里执行 JS = 可调起 start_napcat）。
  // 新增任何动态字段时，凡进 HTML/属性的都要先过这个函数。
  ```
  日志渲染处补一句：
  ```js
  // 安全：日志行可能来自子进程 stdout（含不可信内容），必须先 escapeHtml
  // 再插 innerHTML；<script> 在此只会显示为文本。
  ```

---

#### H3. CSP 为 null，完全无脚本约束

- **位置**：`src-tauri/tauri.conf.json:20-22`
  ```json
  "security": { "csp": null }
  ```
- **问题描述**：`csp: null` 等于不发 CSP。一旦 H2 注入了脚本，它可以不受限制地访问任何 `connect-src`、加载远程脚本、向任意外发数据。
- **修复建议**：
  - 注意：当前前端是**内联 `<script>` + 内联 `onclick` + 内联 style**，直接上 `script-src 'self'` 会让整个应用白屏。正确顺序是：先把 JS 抽到外部文件、内联事件改 `addEventListener`，再开 CSP。
  - 建议的生产 CSP：
    ```json
    "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:6099; object-src 'none'; base-uri 'none'"
    ```
    （WebUI 内嵌页如需加载 `http://127.0.0.1:6099`，按实际需要放进 `connect-src`/`frame-src`。）
- **防护注释建议**（JSON 不支持注释，写进 `tauri.conf.json` 旁的 `SECURITY.md` 或 PR 说明）：
  > CSP 为什么是这个值：script-src 只允许打包进来的本地脚本，杜绝 H2 注入脚本外连；object-src 'none' 禁掉插件；style-src 保留 'unsafe-inline' 是因为主题切换仍用内联 style，后续抽到 CSS 后应移除。

---

### 🟡 中严重度（建议本迭代修）

---

#### M1. 子进程启动路径无任何校验/无来源确认

- **位置**：`src-tauri/src/main.rs:26-35`；前端注释 `dist/index.html:855-856`（自动检测会扫 Downloads/Desktop/D 盘根目录找 `launcher.bat`）
- **问题描述**：`launcher_path` 来自前端，后端不确认文件是否存在、不 canonicalize、不限制扩展名。若自动检测逻辑按注释那样扫描用户 Downloads/Desktop，攻击者只需在 Downloads 放一个伪造的 `launcher.bat`，检测命中后即被执行。
- **修复建议**：路径校验并入 H1；自动检测只“候选展示”，**必须用户点选确认后**才允许 `start_napcat`；日志打印解析后的绝对路径便于排查。
- **防护注释建议**：
  ```rust
  // 安全：launcher_path 不直接信任前端传入，必须 canonicalize 后校验存在性与
  // .bat/.exe 扩展名；自动检测到的路径只作候选，需用户在 UI 确认后才可启动。
  ```

---

#### M2. 敏感信息（OneBot token / 登录态 / 消息体）在日志中明文显示与导出

- **位置**：`dist/index.html:455`（示例日志已出现 `token=sk-9f2a`）、`:666`（“导出”按钮把日志写到 logs/ 目录）、`:743`（UI 声称登录 token“加密保存”，但 main.rs 里**尚无任何落盘代码**）
- **问题描述**：
  - 现在是 demo，但真实接入后 NapCat 子进程 stdout 会带 OneBot accessToken、WebUI token、群消息体、CQ 码。日志在 UI 明文显示（:778）、点“导出”明文落盘。
  - UI 宣称“登录态加密保存到配置目录”，当前代码并不存在该实现——属于未兑现的安全承诺，后续实现时若图省事写明文，风险成立。
- **修复建议**：
  - 渲染/导出日志前用正则脱敏：`token=...`、`Authorization: ...`、`accessToken` 一律打码成 `sk-***`。
  - 登录态落盘时用 Windows DPAPI 或 Tauri 安全存储，文件放 `app_data_dir`，并设置受限 ACL；任何地方不得 `println!`/日志打印。
- **防护注释建议**：
  ```js
  // 安全：子进程 stdout 不可信且可能含 token/消息体，显示与导出前必须先脱敏，
  // 禁止把含 accessToken 的原始日志写进 logs/ 文件。
  ```

---

#### M3. 配置文件安全（QQ 号 / token 落盘）—— 当前未实现，列为待办

- **位置**：当前 `main.rs` 无配置读写代码（`napcat-desktop.json` 仅在前端文案 :576 被提及）
- **问题描述**：快照阶段无落盘风险；但需提前立规矩：敏感字段不进日志、文件权限收紧、不随崩溃转储。
- **修复建议**：后续新增配置模块时，token 类字段与 QQ 号分开存，token 走 DPAPI/keyring。
- **防护注释建议**：
  ```rust
  // 安全：本文件落盘的配置禁止包含明文 accessToken；token 一律走系统凭据库，
  // 且不得被任何日志/错误消息打印。
  ```

---

### 🟢 低严重度（记录即可）

---

#### L1. 关闭最小化 / prevent_close 不能被网页绕过 —— 现状安全

- **位置**：`src-tauri/src/main.rs:79-85`
- **结论**：`CloseRequested` 在 Rust 侧统一 `hide() + prevent_close()`；网页里 JS 调 `window.close()` 同样走这个事件，会被拦下；退出只能来自托盘菜单 `app.exit(0)`（:73）。渲染进程没有任何已注册 command 能直接退出应用。**此实现本身没有可被网页绕过的点**，保持现状即可。
- **防护注释建议**：
  ```rust
  // 行为：点 X 一律最小化到托盘；网页 JS 的 window.close() 也会命中本事件被拦截，
  // 唯一退出入口是托盘菜单。不要在这里新增可被 invoke 触发的退出命令。
  ```

#### L2. 依赖版本无已知高危，但需 CI 里固定审计

- **位置**：`Cargo.toml:11-13`；`Cargo.lock` 实测版本
- **结论**：tauri 2.12.3 / wry 0.57.0 / serde 1.0.229 / serde_json 1.0.154 均为较新补丁版，**截至本审计无已知未修复的高危 CVE**；`Cargo.toml` 用浮动 `version="2"` 是常见写法，实际版本由已提交的 `Cargo.lock` 钉住。
- **建议**：CI 加 `cargo audit`；升级 tauri 时关注 advisory。无需现在改代码。
- **防护注释建议**：在 `Cargo.toml` 顶部：
  ```toml
  # 安全：版本由 Cargo.lock 钉住；CI 跑 cargo audit，发现 tauri/wry 高危 advisory 立即升级。
  ```

#### L3. 非安全类备注（顺带发现）

- `main.rs:31` 的 `#[cfg(target_os = "windows")]` 只包住 `proc` 赋值，会导致非 Windows 平台编译时 `proc` 未定义——不是安全问题，但跨平台构建会挂，建议顺手修。

---

## 二、总结清单

### ✅ 上线前必须修（高）
1. **H1**：`start_napcat` 对 `qq` 做纯数字校验、对 `launcher_path` 做 canonicalize + 扩展名白名单，再 spawn。
2. **H2**：账号/网络/插件视图里所有动态字段（备注、addr、memo、插件 name/desc/author、color）一律 `escapeHtml`，去掉内联 `onclick` 字符串拼接。
3. **H3**：把内联脚本/事件抽到外部文件后，在 `tauri.conf.json` 启用严格 CSP。

### 🔧 建议改进（中）
4. **M1**：自动检测路径仅作候选，用户确认后才启动；打印解析后的绝对路径。
5. **M2**：日志显示/导出前脱敏 token 与消息体；登录态后续用 DPAPI/keyring，不进日志。
6. **M3**：新增配置模块时提前立下“敏感字段不落明文、不打日志”的规矩。

### 📌 保持现状 / 记录（低）
7. **L1**：托盘最小化逻辑安全，勿新增可被 invoke 调用的退出命令。
8. **L2**：依赖无已知高危，CI 加 `cargo audit`。
9. **L3**：修掉 `#[cfg]` 导致的跨平台编译问题。

### 👍 快照中已做对的防护（请不要在重构中回退）
- 日志渲染（`renderLogs` :778 / `renderMiniLog` :783）已统一 `escapeHtml`。
- `toast` 用 `textContent`；`editNick`/`saveNick` 已转义。
- 无 `capabilities/` 插件面，攻击面小。
- 关闭事件统一在 Rust 侧拦截，网页无法自行退出。

---

## v0.4.2 主动审计（对 AI 功能的专项复查）

v0.4.0 引入的 AI 接入 / 自动回复网关属于全新攻击面，本轮做了逐点复查。

### 新发现并已修复

| 编号 | 问题 | 严重度 | 修复 |
|---|---|---|---|
| **N1** | 网关保存表情时，把消息里 `image.url` 直接 `fetch` 并落盘：可被恶意消息用于 **SSRF**（请求内网地址）和**磁盘写满**（无大小上限） | 高 | 增加 QQ CDN 域名白名单（`qpic.cn` / `qq.com` / `gtimg.cn` / `qlogo.cn`）+ 5 MB 上限 + 15 秒超时；非白名单地址记一条日志后跳过 |
| **N2** | `open_external` 仍是 `cmd /C start "" <path>`，仅靠元字符黑名单兜底 | 中 | 改用 **`ShellExecuteW`**，路径直接交给系统 API，**没有 shell 解析层**；黑名单保留作纵深防御 |

### 复查确认无问题

| 位置 | 结论 |
|---|---|
| `pick_directory` 的 PowerShell 拼接 | 用户输入用单引号包裹且 `'` 已翻倍转义，单引号内 `$` 不解析 → 无法注入 |
| `bot.rs` 的 `Win32_Process` 查询 | 脚本是固定字符串，无用户输入参与 |
| `plugin_fs` 的 curl 下载 | 已有 `PLUGIN_URL_HOSTS` 域名白名单 + `validate_plugin_url` |
| 前端 6 处 `innerHTML` | 常量 SVG / 空串 / 已经过 `escapeHtml`，无裸插值 |
| 网关 `stickerPath` / `context` 文件名 | 前者拒绝 `/ \ ..`，后者由 QQ 号/群号（纯数字）拼接 |
| `ai_service` 的 curl 调用 | 密钥走 stdin（`-K -`），不进命令行 |

### 仍存在的已知局限（如实记录）

- 脱敏不覆盖 URL 编码形式与群消息体/CQ 码原文（需引入 regex，刻意不引）
- DPAPI 不防同机同用户的其他进程（Windows 设计如此）
- `install_plugin` 的 zip 解压仍走系统 `tar` / `Expand-Archive`，Zip Slip 依赖其自身实现（已实测拒绝）
