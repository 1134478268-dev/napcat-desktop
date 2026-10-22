// 由 main.rs 拆分生成：窗口控制命令
// 该模块的职责见 docs/ARCHITECTURE.md。

use tauri::Manager;


/// 最小化主窗口。
#[tauri::command]
pub fn win_minimize(app: tauri::AppHandle) -> Result<(), String> {
    let w = app.get_webview_window("main").ok_or("找不到主窗口")?;
    w.minimize().map_err(|e| e.to_string())
}

/// 最大化 / 还原主窗口。
#[tauri::command]
pub fn win_toggle_max(app: tauri::AppHandle) -> Result<(), String> {
    let w = app.get_webview_window("main").ok_or("找不到主窗口")?;
    if w.is_maximized().map_err(|e| e.to_string())? {
        w.unmaximize().map_err(|e| e.to_string())
    } else {
        w.maximize().map_err(|e| e.to_string())
    }
}

/// 关闭主窗口（拦截 CloseRequested 后会最小化到托盘，等同隐藏）。
#[tauri::command]
pub fn win_close(app: tauri::AppHandle) -> Result<(), String> {
    let w = app.get_webview_window("main").ok_or("找不到主窗口")?;
    w.close().map_err(|e| e.to_string())
}
