// 由 main.rs 拆分生成：NapCat 进程拉起（启动形态解析 / spawn）
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::models::*;
use crate::services::*;

/// 解析 quick-login.env（KEY=VALUE，支持 # 注释与空行）为键值对。
/// 单独抽成函数是为了避开"借用临时 String 调 cmd.env"的生命周期坑，
/// 同时把"一行坏数据不影响整份配置"的策略显式化（解析失败的行直接跳过）。
pub fn load_quick_login_env(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // 文件不存在是正常情况（用户没配免扫码登录），静默返回空。
    let Ok(txt) = fs::read_to_string(path) else { return out };
    for line in txt.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        if let Some(i) = t.find('=') {
            let k = t[..i].trim();
            let v = t[i + 1..].trim();
            if !k.is_empty() { out.push((k.to_string(), v.to_string())); }
        }
    }
    out
}

/// 解析启动形态；首启与自动重启共用，保证两条路径参数完全一致。
///
/// 判定顺序：
///   1. <path>\shell\node.exe 与 <path>\shell\index.js 同时存在 → Bootmain（直接起 node 最稳）
///   2. 否则按优先级挑一个 launcher → Script：
///      launcher-user.bat > launcher.bat > start-napcat.bat > napcat.exe
///      > start-napcat.ps1 > shell\napcat.bat
///   3. 都没有 → 报错，并把可接受的启动器列全，方便用户排查。
///
/// === 候选顺序必须与 is_napcat_dir 的识别范围保持同步 ===
/// detect_napcat_path 用 is_napcat_dir 判目录，这里负责真正启动。两处范围一旦脱节，
/// 就会出现"检测说找到 NapCat 了、点启动却提示找不到启动脚本"的割裂（以前只认 4 个
/// 启动器时就是这样）。is_napcat_dir 认得、且调用约定明确的形态，这里都要有对应入口。
///
/// 用 is_file() 而不是 exists()：目录名叫 launcher.bat 的目录也会被 exists() 命中，
/// 但它在 launch_child 里必然过不了扩展名/目录树校验，不如在这里就判掉。
/// 候选里允许出现 shell\ 下的脚本：launch_child 会校验收敛在 napcat_path 目录树内，
/// 而子进程工作目录仍按形态取（Script 形态 = napcat_path）。
pub fn resolve_launch(napcat_path: &str) -> Result<LaunchKind, String> {
    let root = Path::new(napcat_path);
    let shell_dir = root.join("shell");
    let node_exe = shell_dir.join("node.exe");
    let index_js = shell_dir.join("index.js");
    if node_exe.is_file() && index_js.is_file() {
        return Ok(LaunchKind::Bootmain { node_exe, index_js, shell_dir });
    }
    // 顺序即优先级。后两项专门补齐"is_napcat_dir 认得但以前启动不了"的形态：
    //   start-napcat.ps1：launch_child 支持 .ps1（走 pwsh -File），可直接调用；
    //   shell\napcat.bat：bootmain 整合包的 shell 层入口，位于子目录，符合目录树校验。
    // **故意不加** bootmain\NapCatWinBootMain.exe：它的调用约定（是否要传 QQ 号、是否
    // 依赖 bootmain\ 下的 dll/配置）没有可靠依据，猜着传参只会把问题变得更难排查，
    // 这类"认得但起不来"的目录形态留待人工确认后再补。
    let candidates = [
        root.join("launcher-user.bat"),
        root.join("launcher.bat"),
        root.join("start-napcat.bat"),
        root.join("napcat.exe"),
        root.join("start-napcat.ps1"),
        shell_dir.join("napcat.bat"),
    ];
    for p in candidates {
        if p.is_file() { return Ok(LaunchKind::Script { launcher: p }); }
    }
    Err(format!(
        "在 {napcat_path} 下未找到启动脚本；可接受的形式：shell\\node.exe + shell\\index.js（bootmain 整合包）、\
         launcher-user.bat、launcher.bat、start-napcat.bat、napcat.exe、start-napcat.ps1、shell\\napcat.bat"
    ))
}

/// 首启时把磁盘上扫到的插件名列一遍（磁盘层事实）。
/// 真正加载成功/失败要看 NapCat stdout 里的 [plugin] 日志行。
/// 只在首启调用（announce_plugins=true）：自动重启走同一条启动路径，
/// 每次都刷一遍插件清单会把真正的错误日志淹掉。
pub fn announce_plugin_list(app: &tauri::AppHandle, napcat_path: &str) {
    let root = Path::new(napcat_path);
    let shell_plugins = root.join("shell").join("plugins");
    // bootmain 整合包在 shell\plugins，官方包在根目录 plugins，按存在性二选一
    let plugins_dir = if shell_plugins.is_dir() { shell_plugins } else { root.join("plugins") };
    let names: Vec<String> = fs::read_dir(&plugins_dir)
        .map(|rd| rd.flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.') && !n.ends_with(".disabled"))
            .collect())
        .unwrap_or_default();
    let msg = if names.is_empty() {
        format!("{} 目录为空，没有插件", plugins_dir.display())
    } else {
        format!("已检测到 {} 个插件：{}", names.len(), names.join("、"))
    };
    emit_log(app, "plugin", msg);
}

/// 真正 spawn launcher 子进程的内部函数。
///
/// === 防命令注入（安全审计 H1，必须保留这组注释）===
///
/// 1. **为什么不能拼 shell 字符串**：
///    以前的写法是 `cmd /C "{launcher}" {qq}` 拼一整条字符串丢给 cmd.exe。
///    Rust MSVCRT 的 argv 引号规则和 cmd.exe 的引号解析规则不一致，
///    当 launcher_path 或 qq 里带 `"`、`\`、`&` 等字符时，引用会被破掉，
///    攻击者可以通过一个伪造的 QQ 号（例如 `123 & calc.exe`）追加任意命令。
///    所以这里**始终用 args 数组逐个传参**，Command 库会自己处理转义。
///
/// 2. **为什么要 canonicalize + 校验扩展名**：
///    launcher_path 来自配置文件，可能被改成任意路径。我们先 canonicalize
///    成绝对路径、确认文件真实存在、且扩展名只能是 .bat / .ps1 / .exe，
///    拒绝任何含 shell 元字符（& | < > ^）的路径，避免被指向到
///    一个恶意批处理。
///
/// 3. **为什么要约束在 NapCat 目录树内**：
///    只校验扩展名还不够。配置来自 IPC 面（前端 save_config 能改 napcat_path），
///    一旦被滥用，光靠扩展名白名单等于把这里变成"执行任意盘任意 exe/bat/ps1"的通道。
///    所以再要求 canonicalize 之后的 launcher 必须落在 cwd（NapCat 包根目录）之下。
///    canonicalize(cwd) 失败时（目录还没建好等）**跳过**这条校验，保持改动前的行为，
///    不因为一个环境问题把启动功能彻底锁死。
///
/// 4. **qq 为什么必须是纯数字**：
///    start_napcat 调用本函数前已经把 qq 校验成 5-11 位纯数字，
///    这里不再二次校验。即使配置文件被手改，非法 qq 也过不了 start_napcat。
///
/// 5. **.exe 不走 cmd /C**：
///    .bat 是批处理，必须经 cmd.exe 解释；.exe 直接 CreateProcess，
///    多套一层 cmd 反而扩大攻击面。
///
/// stdout/stderr 用 piped() 捕获，交给 attach_log_forwarders 起线程逐行读并 emit
/// 给前端；不能继承父进程 stderr（那样日志会跑到 Tauri 控制台，前端拿不到）。
pub fn launch_child(launcher: &str, qq: &str, cwd: &str, app: &tauri::AppHandle) -> Result<Child, String> {
    // 解析成绝对路径并确认存在。canonicalize 会顺手把符号链接解析掉，
    // 避免被指向一个意外位置。
    let launcher_abs = fs::canonicalize(launcher)
        .map_err(|e| format!("launcher 路径不存在或无法访问: {e}"))?;
    let ext = launcher_abs.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    if ext != "bat" && ext != "exe" && ext != "ps1" {
        return Err(format!("launcher 扩展名必须是 .bat / .ps1 / .exe，实际是 .{ext}"));
    }
    // 二次兜底：拒绝路径里出现 shell 元字符（即便我们用 args 数组，
    // 也加一道白名单，防止未来有人改回字符串拼接）。
    if launcher_abs.to_string_lossy().chars().any(|c| matches!(c, '&' | '|' | '<' | '>' | '^')) {
        return Err("launcher 路径包含非法 shell 元字符".to_string());
    }
    // 目录树约束：launcher 必须位于 NapCat 目录（cwd）之内，防止 IPC 面被滥用成
    // "执行任意盘任意脚本"。canonicalize(cwd) 失败则跳过（见函数头注释第 3 条）。
    if let Ok(root) = fs::canonicalize(cwd) {
        if !launcher_abs.starts_with(&root) {
            return Err(format!("launcher 不在 NapCat 目录内: {}", launcher_abs.display()));
        }
    }
    let launcher_str = launcher_abs.to_string_lossy().to_string();

    let mut cmd = if ext == "bat" {
        // .bat 必须经 cmd.exe 解释。/C 执行完就退出 cmd。
        let mut c = Command::new("cmd");
        c.arg("/C");
        c.arg(&launcher_str);
        c
    } else if ext == "ps1" {
        // .ps1 用 PowerShell 7 (pwsh.exe) 跑。
        // 选 ps1 而不是 bat 的原因：bootmain 整合包的 bat 文件中文编码乱码，
        // 经 cmd 解析时会断裂，bat 立即退出但 ps1 拉起的 node 子进程还在跑，
        // watcher 误判成"崩溃"。直接调 pwsh 绕开 bat 层。
        let pwsh = Path::new(&std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into()))
            .join("PowerShell").join("7").join("pwsh.exe");
        let pwsh_exe = if pwsh.exists() { pwsh } else { PathBuf::from("pwsh.exe") };
        let mut c = Command::new(pwsh_exe);
        c.arg("-NoProfile");
        c.arg("-ExecutionPolicy");
        c.arg("Bypass");
        c.arg("-File");
        c.arg(&launcher_str);
        c
    } else {
        // .exe 直接起进程，不经过 shell，攻击面最小。
        Command::new(&launcher_str)
    };
    if !qq.is_empty() { cmd.arg(qq); }
    cmd.current_dir(cwd);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    // Windows: 不弹出额外的控制台窗口。CREATE_NO_WINDOW = 0x08000000。
    // 这样 node.exe / cmd / pwsh 全部在后台跑，日志通过 stdout 管道回流到
    // 应用内"实时日志"页，用户看不到黑窗。
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn 失败: {e}"))?;
    attach_log_forwarders(&mut child, app);
    Ok(child)
}

/// 统一启动入口：首启与 watcher 自动重启都走这里。
///
/// 为什么必须统一：以前首启和重启各拼一套命令行，结果出现三处不一致
///   - bootmain 首启 cwd 是 shell\，重启却是包根目录（node 找不到 ./config）；
///   - qq 为空时首启不传 -q，重启却无条件传 `-q ""`（NapCat 会当成空账号登录）；
///   - 重启分支没有 stderr 转发、也不认二维码。
///
/// 现在两条路径只有"是否播报插件清单"这一个差别。
///
/// - `napcat_path`：NapCat 包根目录（quick-login.env 与 plugins\ 从这里往下找）；
///   子进程真正的工作目录由 kind.cwd() 决定（bootmain 是 shell\）。
/// - `announce_plugins`：首启 true（列一遍磁盘上的插件名），自动重启 false（避免刷屏）。
pub fn spawn_napcat_process(
    kind: &LaunchKind, qq: &str, napcat_path: &str, app: &tauri::AppHandle, announce_plugins: bool,
) -> Result<Child, String> {
    let cwd = kind.cwd(napcat_path);
    let mut child = match kind {
        LaunchKind::Bootmain { node_exe, index_js, .. } => {
            let mut cmd = Command::new(node_exe);
            cmd.arg(index_js);
            // 只在 qq 非空时追加 -q <qq>：传 `-q ""` 会让 NapCat 认为要登录空账号，
            // 比不带参数更容易直接失败。首启与重启现在共用这一条判断。
            if !qq.is_empty() {
                cmd.arg("-q");
                cmd.arg(qq);
            }
            cmd.current_dir(&cwd);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            // 不弹外部控制台窗口（CREATE_NO_WINDOW = 0x08000000）
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            // quick-login.env 只在 bootmain 形态有意义：它给 NapCat 提供免扫码账号 /
            // 快速登录密码。解析成键值对后逐个注入环境变量。
            for (k, v) in load_quick_login_env(&Path::new(napcat_path).join("quick-login.env")) {
                cmd.env(k, v);
            }
            cmd.spawn().map_err(|e| format!("启动 node.exe 失败: {e}"))?
        }
        // 官方 / 通用形态：走 launcher 脚本。launch_child 内部已经挂了日志转发
        // （含脱敏/分级/二维码），所以这里不重复挂。
        LaunchKind::Script { launcher } => launch_child(&launcher.to_string_lossy(), qq, &cwd, app)?,
    };
    if announce_plugins { announce_plugin_list(app, napcat_path); }
    // Bootmain 分支是自己 spawn 的，日志转发要在这里挂。Script 分支的转发已在
    // launch_child 内挂好（stdout/stderr 只能 take 一次，这里再挂也是空操作，
    // 但仍按形态显式区分，避免读代码时误以为漏了）。
    if matches!(kind, LaunchKind::Bootmain { .. }) {
        attach_log_forwarders(&mut child, app);
    }
    Ok(child)
}
