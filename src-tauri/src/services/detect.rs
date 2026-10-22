// 由 main.rs 拆分生成：NapCat / QQNT 安装路径探测
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::path::{Path, PathBuf};


/// 判断目录是否像 NapCat 安装目录。
/// 支持多种打包形态：
///   1. 官方 NapCat.Shell.zip 解压：根目录 launcher-user.bat / launcher.bat
///   2. bootmain 整合包（带 NapCatWinBootMain.exe）：根目录 start-napcat.bat，
///      子目录 bootmain\NapCatWinBootMain.exe 和 shell\napcat.bat
///   3. 通用：napcat.ps1 / napcat.exe / shell\launcher.bat / config\ 目录
///
/// 注意：这里认得的形态必须和 resolve_launch 的候选保持同步，否则会出现
/// "检测得到但启动不了"的割裂（详见 resolve_launch 的注释）。
pub fn is_napcat_dir(d: &Path) -> bool {
    d.join("launcher-user.bat").exists()
        || d.join("launcher.bat").exists()
        || d.join("start-napcat.bat").exists()
        || d.join("start-napcat.ps1").exists()
        || d.join("napcat.ps1").exists()
        || d.join("napcat.exe").exists()
        || d.join("bootmain").join("NapCatWinBootMain.exe").exists()
        || d.join("shell").join("napcat.bat").exists()
        || d.join("shell").join("launcher.bat").exists()
        || d.join("config").exists()
}

/// 在 D:\ 根目录下找名字含 "napcat" 的子目录，验证标志文件后加入候选。
/// 不递归深层目录——递归扫全盘会阻塞 Tauri 事件循环（前端卡死）。
/// 理由：用户安装 NapCat 时目录名通常带 napcat 字样，D:\ 根目录一次 read_dir 就够。
pub fn shallow_scan_napcat(root: &Path, _max_depth: usize, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(root) { Ok(e) => e, Err(_) => return };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() { continue; }
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_lowercase(),
            None => continue,
        };
        // 只看名字里带 napcat 的目录，其他全部跳过，避免遍历全盘
        if !name.contains("napcat") && !name.contains("nap") { continue; }
        if is_napcat_dir(&p) { out.push(p); }
    }
}

/// 从注册表读 QQNT 安装路径。参考官方 NapCat-Installer/script/install.ps1：
///   1. HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\QQ
///      读 UninstallString，取其目录，验证 QQ.exe 存在
///   2. HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT 的 Install 键
///
/// 用 reg.exe query，避免引入 winreg crate。
pub fn read_reg_qqnt() -> Option<PathBuf> {
    // 1. Uninstall 项：从 UninstallString 推安装目录
    if let Ok(out) = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\QQ"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(idx) = line.find("UninstallString") {
                // 格式：UninstallString    REG_SZ    C:\Program Files\Tencent\QQNT\uninst.exe
                let rest = &line[idx..];
                if let Some(sz) = rest.find("REG_SZ") {
                    let path = rest[sz + 6..].trim().trim_matches('"').to_string();
                    if !path.is_empty() {
                        if let Some(parent) = Path::new(&path).parent() {
                            if parent.join("QQ.exe").exists() { return Some(parent.to_path_buf()); }
                        }
                    }
                }
            }
        }
    }
    // 2. Tencent\QQNT Install 键
    if let Ok(out) = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT", "/v", "Install"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(sz) = line.find("REG_SZ") {
                let path = line[sz + 6..].trim().trim_matches('"').to_string();
                if !path.is_empty() { return Some(PathBuf::from(path)); }
            }
        }
    }
    None
}
