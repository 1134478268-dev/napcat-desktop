# 变更日志

本文件记录每个版本的变更。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 计划中
- AI 工具调用（function calling）：让 AI 直接发消息、查群成员
- 上下文自动摘要成长期记忆
- 网关多账号支持

## [0.4.2] - 2026-10-22

对 v0.4.0 引入的 AI 功能做专项安全复查，修掉两个问题。

### 安全

- **N1（高）AI 网关 SSRF / 磁盘写满**：保存表情时会把消息里的 `image.url` 直接请求并落盘。
  恶意消息可借此请求内网地址或写入超大文件。现增加 **QQ CDN 域名白名单**
  （`qpic.cn` / `qq.com` / `gtimg.cn` / `qlogo.cn`）+ **5 MB 上限** + 15 秒超时，
  非白名单地址记日志后跳过
- **N2（中）`open_external` 去除 cmd 二次解析**：从 `cmd /C start "" <path>` 改为
  **`ShellExecuteW`**，路径直接交给系统 API，没有 shell 解析层；元字符黑名单保留作纵深防御

### 复查确认无问题

`pick_directory` 的 PowerShell 拼接（单引号翻倍转义）、`bot.rs` 的固定脚本、
`plugin_fs` 的下载白名单、前端 6 处 `innerHTML`、网关的路径拼接、AI 请求的密钥传递方式。

## [0.4.1] - 2026-10-22

本版是一次安全收口，把审计报告里遗留的 4 项全部闭环。

### 安全

- **M3 配置加密**：`api_key` 与 OneBot `access_token` 改用 **Windows DPAPI** 加密落盘（`dpapi:<hex>`），
  读写自动加解密，旧明文配置原样兼容并在下次保存时升级
- **M2 脱敏增强**：键表补齐 `client_secret` / `clientsecret` / `private_key` / `privatekey`；
  驼峰写法（`accessToken` / `apiKey`）由大小写不敏感匹配覆盖；新增单测锁定
- **H3 CSP 收紧**：`script-src` 去掉 `'unsafe-inline'`（前端已无内联脚本/事件/`javascript:`）
- **L2 依赖审计**：CI 增加 `rustsec/audit-check`

### 验证

- 脱敏单测 8 个、DPAPI 单测 3 个（含往返加解密）
- 落盘实测：`api_key` 从 `sk-test-...` 变为 `dpapi:01000000...`
- 重启后 AI 页正常解密显示，CSP 收紧后界面与 IPC 均正常

## [0.4.0] - 2026-10-22

本版本把「能跑」推进到「可长期用」：日志能分开看、AI 能整体关、启动不再撞车。

### 新增

- **AI 接入总开关**：一键停用全部 AI 能力（对话、网关、调用），配置与人设/记忆保留
- **日志按来源分流**：`napcat://log` 事件增加 `source` 字段（`napcat` / `ai` / `bot`）；
  前端日志页可下拉筛选，落盘拆成 `error.log` 与 `ai-error.log`
- **启动前清理残留进程**：自动检测并结束上次残留的 NapCat 进程树，避免「账号已登录」撞车
- **提交信息规范钩子**：`.githooks/commit-msg` 校验 Conventional Commits
- **Git 使用指南**：`docs/GIT_GUIDE.md`
- **CI 工作流**：`.github/workflows/ci.yml`（Windows + clippy + 测试）

### 修复

- 启动时若存在残留 NapCat 进程，会提示「当前账号已登录，无法重复登录」而无法启动
- AI 表单中「群聊仅 @ 时」等开关的复选框被 76px 宽度挤压、文字换行错位
- 桌面端退出时不清理 AI 网关，留下孤儿进程（下次启动会变成双网关重复回复）
- `ai_save_config` 成功后返回 `null`，前端用 `!== null` 判成功导致流程静默中断
- AI 网关连接 WebSocket 未带 `access_token`，被 NapCat 拒绝后无限重连
- AI 配置加载失败时前端用空默认值覆盖磁盘，导致 providers/人设/记忆丢失

## [0.3.0] - 2026-10-22

### 新增

- **AI 自动回复网关**：独立 Node 进程，连 NapCat 正向 WS 收消息、调 AI、回消息
- **会话上下文**：按会话存 `ai/contexts/<type>_<id>.json`，保留最近 N 轮
- **本地表情库**：收到的图片自动下载到 `ai/stickers/`，AI 可用 `[sticker:名字]` 发送
- **启动即运行**：`gateway.auto_start` 时桌面端 setup 阶段自动拉起

## [0.2.0] - 2026-10-22

### 新增

- **AI 接入页**：多厂商预设、三种协议族（openai / anthropic / gemini）
- **人设**：多套系统提示词，可切换
- **长期记忆**：手动维护，每次对话注入
- **对话测试**：支持 markdown 代码块渲染
- **密钥安全**：请求走 `curl -K -`，凭据从 stdin 传入，不出现在命令行

## [0.1.0] - 2026-10-22

### 新增

- NapCat 进程管理（启动形态解析、崩溃指数退避重启、日志脱敏转发）
- QQ 账号管理、OneBot 网络配置
- 插件浏览、拖放安装、可选插件一键安装
- 内嵌 NapCat WebUI
- 系统托盘、双主题、无边框窗口
- 分层重构：前端 core/ui/views，后端 models/services/commands/utils
- 架构加固：锁中毒恢复、事件统一入口、零 clippy 警告

[Unreleased]: https://example.com/compare/v0.4.0...HEAD
[0.4.0]: https://example.com/compare/v0.3.0...v0.4.0
[0.3.0]: https://example.com/compare/v0.2.0...v0.3.0
[0.2.0]: https://example.com/compare/v0.1.0...v0.2.0
[0.1.0]: https://example.com/releases/tag/v0.1.0
