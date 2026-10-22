// 由 main.rs 拆分生成：配置与日志路径解析
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::PathBuf;
use tauri::Manager;


/// 返回 napcat-desktop.json 的绝对路径。
///
/// 为什么放 app_config_dir：
///   - 这是 Tauri 约定的"每用户、每应用"配置目录（Windows 下是
///     %APPDATA%\com.napcat.desktop\ 之类），不需要管理员权限，
///     不会被程序卸载时误删；
///   - 不能放在 exe 同目录，因为 NSIS 安装版会装到 Program Files，
///     普通用户没有写权限。
pub fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| format!("无法获取 app_config_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    Ok(dir.join("napcat-desktop.json"))
}

/// 返回 logs/ 下某个日志文件的绝对路径，目录不存在时创建。
pub fn log_file_path(app: &tauri::AppHandle, filename: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| format!("无法获取 app_config_dir: {e}"))?;
    let logs = dir.join("logs");
    fs::create_dir_all(&logs).map_err(|e| format!("创建日志目录失败: {e}"))?;
    Ok(logs.join(filename))
}

/// 返回 error.log 的绝对路径（NapCat 与应用自身的 err/warn）。
pub fn error_log_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    log_file_path(app, "error.log")
}

/// 返回 ai-error.log 的绝对路径（AI 网关与 AI 调用的 err/warn）。
///
/// 为什么和 error.log 分开：排查时两者的噪声完全不同 ——
/// NapCat 的报错通常是登录/协议/插件，AI 的报错通常是 key、限流、网络超时。
/// 混在一个文件里，翻问题时得先按前缀过滤，夜里更容易看漏。
pub fn ai_error_log_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    log_file_path(app, "ai-error.log")
}
