# 变更日志

本文件记录每个版本的变更。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 计划中
- AI 工具调用（function calling）：让 AI 直接发消息、查群成员
- 上下文自动摘要成长期记忆
- 网关多账号支持

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
