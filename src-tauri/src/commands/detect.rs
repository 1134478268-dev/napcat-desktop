// 由 main.rs 拆分生成：路径探测命令
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::path::{Path, PathBuf};

use crate::services::*;

/// 扫描常见位置与 D 盘浅层目录，返回所有疑似 NapCat 安装路径。
///
/// 判定标志（任一命中即视为找到）：
///   - launcher.bat / napcat.ps1 / napcat.exe 直接在目录下
///   - 子目录 shell/launcher.bat（NapCat.Shell 解压结构）
///   - 子目录 config/ 下存在 napcat_*.json
///
/// 扫描策略：
///   1. 硬编码常见安装位（Program Files / D:\Apps / 用户目录等）；
///   2. 浅层扫描 D:\ 根目录，只读名字含 napcat 的目录，命中即停，避免整盘递归卡死。
#[tauri::command]
pub fn detect_napcat_path() -> Vec<String> {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let pf = std::env::var("ProgramFiles").unwrap_or_default();
    let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();

    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from(r"D:\napcat"),
        PathBuf::from(r"D:\NapCat"),
        PathBuf::from(r"D:\napcat-shell"),
        PathBuf::from(r"D:\NapCat.Shell"),
        PathBuf::from(r"D:\NapCatQQ"),
        PathBuf::from(r"D:\Apps\NapCat.Shell"),
        PathBuf::from(r"D:\Apps\NapCat"),
        PathBuf::from(r"C:\napcat"),
        PathBuf::from(r"C:\NapCat"),
        Path::new(&pf).join("napcat"),
        Path::new(&pf).join("NapCat"),
        Path::new(&pf86).join("napcat"),
        Path::new(&local).join("Programs").join("napcat"),
        Path::new(&appdata).join("napcat"),
    ];

    // 浅层扫描 D:\ 下 2-3 层目录找 launcher.bat / napcat.exe
    shallow_scan_napcat(&PathBuf::from(r"D:\"), 3, &mut candidates);

    let mut out: Vec<String> = Vec::new();
    for d in candidates {
        if is_napcat_dir(&d) { out.push(d.to_string_lossy().to_string()); }
    }
    out.dedup();
    out
}

/// 检测 QQNT（NTQQ）安装路径。
///
/// 顺序：
///   1. 注册表 HKLM\SOFTWARE\Tencent\QQNT InstallPath
///   2. 注册表 HKCU\Software\Tencent\QQNT
///   3. 常见安装位（C/D 盘 Program Files\Tencent\QQNT）
/// 判定：目录下存在 QQ.exe。
#[tauri::command]
pub fn detect_qq_path() -> Vec<String> {
    let pf = std::env::var("ProgramFiles").unwrap_or_default();
    let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();

    let mut candidates: Vec<PathBuf> = vec![
        Path::new(&pf).join("Tencent").join("QQNT"),
        Path::new(&pf86).join("Tencent").join("QQNT"),
        PathBuf::from(r"D:\Program Files\Tencent\QQNT"),
        PathBuf::from(r"D:\Program Files (x86)\Tencent\QQNT"),
        PathBuf::from(r"D:\Tencent\QQNT"),
        PathBuf::from(r"C:\Tencent\QQNT"),
    ];

    // 读注册表
    if let Some(p) = read_reg_qqnt() { candidates.push(p); }

    let mut out: Vec<String> = Vec::new();
    for d in candidates {
        if d.join("QQ.exe").exists() { out.push(d.to_string_lossy().to_string()); }
    }
    out.dedup();
    out
}

/// 弹一个系统"浏览文件夹"对话框，让用户选目录。
/// 用 PowerShell + System.Windows.Forms.FolderBrowserDialog，无需引入 Tauri dialog 插件。
/// 返回用户选的目录路径；取消返回空串。
#[tauri::command]
pub fn pick_directory(default_path: Option<String>) -> Result<String, String> {
    let start = default_path.unwrap_or_else(|| "D:\\".to_string());
    // PowerShell 脚本：弹 FolderBrowserDialog，把选中路径写到 stdout。
    // 路径里的 ' 自身翻倍转义，避免把用户输入拼成另一条命令。
    let ps = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms | Out-Null; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = '选择目录'; $f.ShowNewFolderButton = $true; if (Test-Path '{0}') {{ $f.SelectedPath = '{0}' }}; $r = $f.ShowDialog(); if ($r -eq 'OK') {{ Write-Output $f.SelectedPath }}"#,
        start.replace('\'', "''")
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
        .map_err(|e| format!("无法启动目录选择对话框: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
