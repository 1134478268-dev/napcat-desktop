// 由 main.rs 拆分生成：插件目录扫描、下载、解压、复制
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;


/// 从 napcat_path 推断插件目录。
/// 优先级：
///   1. <napcat_path>\shell\plugins\   （bootmain 整合包结构）
///   2. <napcat_path>\plugins\         （官方 NapCat.Shell 结构）
///
/// 都不存在则返回第一个（自动创建）。
/// 注意：napcat_path 为空时返回空串，调用方必须自己判空（见 ensure_plugins_dir）。
pub fn resolve_plugins_dir(napcat_path: &str) -> String {
    if napcat_path.is_empty() { return String::new(); }
    let p = Path::new(napcat_path);
    let candidates = [p.join("shell").join("plugins"), p.join("plugins")];
    for c in &candidates {
        if c.is_dir() { return c.to_string_lossy().to_string(); }
    }
    // 默认返回 shell\plugins（bootmain 整合包的标准位置），不存在则自动创建
    let target = candidates[0].clone();
    let _ = fs::create_dir_all(&target);
    target.to_string_lossy().to_string()
}

/// 校验插件目录可用：必须非空且是绝对路径。
///
/// 为什么必须校验（空目录漏洞）：resolve_plugins_dir(&cfg.napcat_path) 在内存配置还没
/// 加载（napcat_path 为空）时返回空串，PathBuf::from("") 会退化成一个**相对路径**，
/// 于是 join(name) 相对于"进程当前目录"——插件会被复制/读取到安装目录里
/// （Program Files 下还可能直接失败或写脏文件）。这里必须给出可读的错误，
/// 让用户先去设置页配好 NapCat 路径，而不是默默写错地方。
pub fn ensure_plugins_dir(raw: &str) -> Result<PathBuf, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("插件目录未配置，请先在设置页设置 NapCat 路径".to_string());
    }
    let p = PathBuf::from(t);
    if !p.is_absolute() {
        return Err(format!("插件目录不是绝对路径（{t}），请重新设置 NapCat 路径"));
    }
    Ok(p)
}

/// 单个 package.json 的解析上限：1 MiB。
/// 插件目录内容不受控（用户拖进来的第三方包），遇到超大/损坏的 package.json
/// 直接跳过解析，避免一次列表扫描被一个文件拖住。
pub const MAX_PACKAGE_JSON_BYTES: u64 = 1024 * 1024;

/// 一键安装允许的下载域名白名单。
///
/// 为什么必须白名单：这是本项目**唯一**一条"由前端给出 URL、后端去联网取文件"的通道。
/// 不限制就等于把"下载任意远程文件到本机"的能力交给了渲染进程（叠加任何 HTML 注入 = 任意文件落地）。
/// 只放行 GitHub 生态里确实会用到的域名。特别注意 `objects.githubusercontent.com`：
/// release 资产在 302 之后的**真实落地域名**，国内直连 github.com 常常超时，但该域名通常可达。
pub const PLUGIN_URL_HOSTS: [&str; 5] = [
    "github.com",
    "codeload.github.com",
    "objects.githubusercontent.com",
    "raw.githubusercontent.com",
    "api.github.com",
];

/// GitHub 加速镜像前缀：国内直连 github.com 不稳定时的备选通路。
///
/// 为什么内置而不是让用户自己配：实测同一 URL「一次成功、一次 20 秒超时」，
/// 而这三个镜像在同样环境下都稳定返回（均已实测下载成功）。
/// 只在**直连失败之后**才使用，且仅用于 `https://github.com/` 开头的地址。
/// 代价：镜像会中转请求，因此不把它当首选，且成功来源会写进 error.log 便于追溯。
pub const GITHUB_MIRRORS: [&str; 3] = [
    "https://ghproxy.net/",
    "https://gh-proxy.com/",
    "https://ghfast.top/",
];

/// 用 curl 下载一个 URL 到 out，失败返回可读错误。
/// 抽成独立函数，是为了让"直连 → 多个镜像依次重试"的流程保持扁平。
pub fn download_via_curl(url: &str, out: &Path, proxy: &str) -> Result<(), String> {
    let mut args: Vec<String> = vec![
        "-L".into(), "--fail".into(), "--silent".into(), "--show-error".into(),
        "--connect-timeout".into(), "20".into(), "--max-time".into(), "300".into(),
        "-o".into(), out.to_string_lossy().to_string(),
    ];
    if !proxy.trim().is_empty() {
        args.push("--proxy".into());
        args.push(proxy.trim().to_string());
    }
    args.push(url.to_string());

    let mut cmd = Command::new("curl");
    cmd.args(&args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW，别弹黑框
    }
    let dl = cmd.output().map_err(|e| format!("无法启动 curl: {e}"))?;
    if !dl.status.success() {
        let err = String::from_utf8_lossy(&dl.stderr).trim().to_string();
        return Err(format!(
            "curl 退出码 {:?}：{}",
            dl.status.code(),
            if err.is_empty() { "无错误输出".to_string() } else { err }
        ));
    }
    Ok(())
}

/// 校验一键安装的 URL：必须 https，且主机名在白名单内。
/// 只做字符串解析（不引 url crate）：取 "://" 之后到第一个分隔符之间的部分作为主机名，
/// 再去掉可能的 `user@` 与 `:port`，避免 `github.com@evil.com` 这类花招绕过前缀判断。
pub fn validate_plugin_url(url: &str) -> Result<(), String> {
    let rest = url.strip_prefix("https://")
        .ok_or_else(|| "只允许 https:// 开头的下载地址".to_string())?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() || !PLUGIN_URL_HOSTS.contains(&host.as_str()) {
        return Err(format!("下载地址的域名不在白名单内: {host}"));
    }
    Ok(())
}

/// 解压 zip。拖放安装与一键安装共用这一条路径。
///
/// **首选系统自带的 `tar.exe`（Win10 1803+ 内置 bsdtar，原生支持 zip）**，理由：
///   1) 它不依赖 PowerShell 模块。`Expand-Archive` 属于 `Microsoft.PowerShell.Archive` 模块，
///      实测在"宿主进程由 PowerShell 7 启动"这种情况下，`powershell.exe`(5.1) 会报
///      "未找到模块 Microsoft.PowerShell.Archive 中的 Expand-Archive 命令，或者无法加载该模块"，
///      于是拖放安装 zip 和一键安装**全部失败**——这个坑是实际踩到的，错误已落在 error.log；
///   2) tar 直接接收参数数组，不经过任何 shell 解释，可控路径不需要再做引号转义。
///
/// 只有 tar 不可用（极老的系统）或 tar 解压失败时，才回退到 PowerShell，并显式带上
/// `-ExecutionPolicy Bypass`，避免受脚本执行策略影响导致模块加载失败。
pub fn expand_zip(zip: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("创建解压目录失败: {e}"))?;

    // 不预先初始化：match 的每个非成功分支都会赋值，
    // 预初始化反而会触发 unused_assignments（初始值永远不会被读到）
    
    let tar_err: String = match Command::new("tar")
        .args(["-xf", &zip.to_string_lossy(), "-C", &dest.to_string_lossy()])
        .output()
    {
        Ok(o) if o.status.success() => return Ok(()),
        Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
        Err(e) => format!("无法启动 tar: {e}"),
    };

    // 回退：老系统没有 tar.exe 时用 PowerShell 的 Expand-Archive
    let out = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
            &format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                zip.to_string_lossy().replace('\'', "''"),
                dest.to_string_lossy().replace('\'', "''"))])
        .output()
        .map_err(|e| format!("解压失败（tar: {tar_err}；也无法启动 PowerShell: {e}）"))?;
    if !out.status.success() {
        let ps_err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("解压失败：tar: {tar_err}；Expand-Archive: {ps_err}"));
    }
    Ok(())
}

/// 展平"套了一层目录"的 zip。
///
/// 为什么需要：GitHub 的 release 资产与源码 zip 习惯把所有文件放在一个顶层目录里
/// （例如解压得到 `napcat-plugin-cleaner/package.json`），而 NapCat 只会加载
/// `plugins/<插件名>/` 下**直接**暴露 package.json 的插件。不展平的话装完是个"套娃目录"，
/// 面板也扫不到，等于白装。
/// 判定：解压结果里只有一个条目、且它是目录 → 把该目录里的内容上移一层再删掉它。
pub fn flatten_single_dir(dir: &Path) -> Result<(), String> {
    let entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("读取解压结果失败: {e}"))?
        .flatten().map(|e| e.path()).collect();
    if entries.len() != 1 || !entries[0].is_dir() { return Ok(()); }
    let inner = entries[0].clone();
    for e in fs::read_dir(&inner).map_err(|e| format!("读取内层目录失败: {e}"))?.flatten() {
        let from = e.path();
        let to = dir.join(e.file_name());
        // rename 同盘极快；跨卷会失败，此时退化成复制
        if fs::rename(&from, &to).is_err() {
            if from.is_dir() {
                copy_dir_recursive(&from, &to, 0).map_err(|err| format!("展平目录失败: {err}"))?;
                let _ = fs::remove_dir_all(&from);
            } else {
                fs::copy(&from, &to).map_err(|err| format!("展平目录失败: {err}"))?;
                let _ = fs::remove_file(&from);
            }
        }
    }
    let _ = fs::remove_dir(&inner);
    Ok(())
}

/// 递归复制的最大深度。正常插件目录不会有几十层，
/// 超过这个深度基本可以断定是符号链接/junction 形成的环。
pub const MAX_COPY_DEPTH: u32 = 32;

/// 递归复制文件夹。
///
/// 安全边界：
///   - 跳过符号链接 / junction：既不复制链接本身（复制出来是坏链接），也避免沿着
///     链接递归到目录树之外、或者链接成环导致无限递归；
///   - 限制递归深度（MAX_COPY_DEPTH）：即使遇到 file_type() 判不出来、伪装成普通
///     目录的 reparse point，也只会返回明确错误，而不是栈溢出崩溃。
pub fn copy_dir_recursive(src: &Path, dst: &Path, depth: u32) -> std::io::Result<()> {
    if depth > MAX_COPY_DEPTH {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,
            format!("目录层级超过 {MAX_COPY_DEPTH} 层，疑似符号链接环，已中止复制")));
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let e = entry?;
        let t = e.file_type()?;
        // Windows 下 junction / 目录符号链接同样会被 is_symlink 命中（按 reparse tag 判定）
        if t.is_symlink() { continue; }
        let from = e.path();
        let to = dst.join(e.file_name());
        if t.is_dir() {
            copy_dir_recursive(&from, &to, depth + 1)?;
        } else if t.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
