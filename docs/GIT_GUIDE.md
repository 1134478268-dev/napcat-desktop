# Git 使用规范

## 仓库约定

- 主分支 `main`，始终保持可构建、可运行。
- 功能开发开短分支，合回 `main` 后删除。
- 提交信息由 `.githooks/commit-msg` 校验（Conventional Commits），不合规会被拒绝。

首次克隆后需要启用钩子（钩子随仓库分发，但 `core.hooksPath` 是本机配置）：

```powershell
git config core.hooksPath .githooks
```

## 提交信息格式

```
<type>(<scope>): <subject>
```

| type | 用途 |
| --- | --- |
| `feat` | 新功能 |
| `fix` | 修 bug |
| `docs` | 只改文档 |
| `style` | 格式 / 空白，不改逻辑 |
| `refactor` | 重构，不改行为 |
| `perf` | 性能 |
| `test` | 测试 |
| `build` | 构建脚本 / 依赖 |
| `ci` | CI 配置 |
| `chore` | 杂项 |
| `revert` | 回滚 |

`scope` 建议用目录名：`frontend`、`backend`、`ai`、`gateway`、`docs`、`scripts`。

示例：

```
feat(ai): 网关支持表情包发送
fix(frontend): 修复保存成功后流程中断
refactor(backend): 把锁访问收敛到 utils::lock
docs: 补充网关行为约定
```

临时跳过检查：`git commit --no-verify`（仅在明确知道原因时用）。

## 分支

```powershell
# 开功能分支
git switch -c feat/ai-context-summary

# 开发、提交若干次后合回 main
git switch main
git merge --no-ff feat/ai-context-summary
git branch -d feat/ai-context-summary
```

## 提交前检查

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\lint.ps1
```

它跑 clippy（警告即失败）+ Rust 单测 + 前端单测。**建议只在通过后提交**，否则 `main` 会出现「构建不了」的提交。

构建产物检查：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build.ps1   # 会更新根目录 NapCatDesktop.exe
git status                                                     # 该文件已被 .gitignore 忽略，不应出现在变更里
```

## 哪些东西不入库

| 内容 | 位置 | 原因 |
| --- | --- | --- |
| 编译产物 | `src-tauri/target/`、`NapCatDesktop.exe` | 可由 `build.ps1` 重建，体积大 |
| Tauri 生成物 | `src-tauri/gen/schemas/` | 构建时自动生成 |
| 运行数据 | `%APPDATA%\com.napcat.desktop\ai\` | 含 API Key、记忆、上下文，**绝不入库** |
| 日志 / 临时文件 | `*.log`、`*.tmp` | 噪音 |
| 本地环境 | `.vscode/`、`.env` | 因人而异 |

> 密钥只存在于 `%APPDATA%` 下的 `ai/config.json`，项目目录里没有任何凭据。
> 如果以后要加示例配置，用 `.env.example` 这种占位文件（`.gitignore` 已放行）。

## 常用操作

```powershell
# 看最近改动
git log --oneline -10
git show --stat HEAD

# 撤销工作区改动（未 add）
git restore <file>

# 取消暂存
git restore --staged <file>

# 修改上一次提交（还没推送时）
git commit --amend

# 回滚到某个提交（保留改动为未暂存）
git reset --soft <commit>

# 只想撤销某一次提交的改动
git revert <commit>        # 生成一个反向提交，历史可追溯（推荐）

# 查某行代码是谁、哪次提交改的
git blame -L 40,60 src-tauri/src/services/ai_gateway.rs
```

## 标签与发布

```powershell
# 打完包、验证通过后打标签
git tag -a v0.3.0 -m "AI 自动回复网关"
git show v0.3.0
```

版本号与 `src-tauri/tauri.conf.json` 的 `version` 保持一致。

## 常见问题

**提交时提示「提交信息不符合规范」**
按 `<type>(<scope>): <subject>` 改写第一行；确实要跳过加 `--no-verify`。

**`git status` 里出现 `NapCatDesktop.exe`**
说明 `.gitignore` 被改坏了，恢复：

```powershell
git checkout -- .gitignore
```

**行尾警告（LF will be replaced by CRLF）**
由 `.gitattributes` 控制：源码统一 LF，`.bat` / `.ps1` 保持 CRLF。若大量出现，执行一次：

```powershell
git add --renormalize .
git commit -m "chore: 统一行尾"
```

## 钩子说明

仓库自带三个钩子（`.githooks/`），通过 `core.hooksPath` 生效：

| 钩子 | 时机 | 内容 | 耗时 |
| --- | --- | --- | --- |
| `commit-msg` | 每次提交 | 校验 Conventional Commits 格式 | 瞬间 |
| `pre-commit` | 每次提交 | `cargo check` + 前端单测 | 5-10 秒 |
| `pre-push` | 每次推送 | 完整 lint（clippy 警告即失败 + 全部测试） | 40-90 秒 |

这样分层的原因：提交频繁、推送少。把完整 lint 挂在每次提交上，人会习惯性
`--no-verify`，反而失去意义。

单独跳过某一层：

```powershell
git commit --no-verify      # 跳过 commit-msg + pre-commit
git push --no-verify        # 跳过 pre-push
```

## 版本发布流程

1. **更新 CHANGELOG.md**：把 `[Unreleased]` 下的内容整理到新的版本号下。
2. **同步版本号**（两处必须一致）：
   - `src-tauri/Cargo.toml` 的 `version`
   - `src-tauri/tauri.conf.json` 的 `version`
   （`Cargo.lock` 会在下次构建时自动更新，记得一起提交）
3. **提交**：
   ```powershell
   git add -A
   git commit -m "chore(release): v0.4.0"
   ```
4. **打标签**（附注标签，带说明）：
   ```powershell
   git tag -a v0.4.0 -m "AI 网关 + 日志分流 + 总开关"
   git show v0.4.0 --stat
   ```
5. **推送**（标签要单独推）：
   ```powershell
   git push origin main
   git push origin --tags
   ```

### 版本号怎么定

| 变化 | 版本位 | 例子 |
| --- | --- | --- |
| 不兼容的接口/配置变更 | major | 1.0.0 |
| 向下兼容的新功能 | minor | 0.4.0 → 0.5.0 |
| 向下兼容的修复 | patch | 0.5.0 → 0.5.1 |

> 配置结构变更（如 `AppConfig` 删字段）属于不兼容变更，必须进 major 或至少 minor，
> 并在 CHANGELOG 里写明迁移方式。

## 回滚到某个版本

```powershell
# 查看某个 tag 的内容
git show v0.3.0 --stat

# 从该标签切一个修复分支（推荐，不动 main）
git switch -c hotfix/from-v0.3.0 v0.3.0

# 或者直接在 main 上反向提交某次改动
git revert <commit>
```
