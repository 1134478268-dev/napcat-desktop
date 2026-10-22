//! Tauri 事件发送统一入口 + 事件名常量。
//!
//! 为什么集中：这 3 个事件名是 frontend 的硬契约，散落成字面量时改一个漏一个，
//! 而且每条日志都要手写一遍 `json!({"line": ..., "level": ...})`，
//! 曾经因为某个出口漏了 `qq` 键导致前端账号卡片状态漂移。
//! 统一后：事件名只在这里定义，日志发送只接受 (level, line) 两个参数。

use tauri::Emitter;

/// 日志来源。前端据此做筛选，落盘也据此分文件。
pub const SRC_NAPCAT: &str = "napcat";
pub const SRC_AI: &str = "ai";
pub const SRC_BOT: &str = "bot";

/// 日志行。payload: `{ line, level, source }`
/// level ∈ info/ok/warn/err/recv/plugin；source ∈ napcat/ai/bot
pub const EVT_LOG: &str = "napcat://log";
/// 运行状态。payload 见 [`crate::services::watcher::emit_status`]。
pub const EVT_STATUS: &str = "napcat://status";
/// 登录二维码。payload: `{ data_url: string }`
pub const EVT_QRCODE: &str = "napcat://qrcode";

/// 推送一行日志到前端（默认来源 napcat）。
pub fn emit_log(app: &tauri::AppHandle, level: &str, line: impl Into<String>) {
    emit_log_from(app, SRC_NAPCAT, level, line);
}

/// 推送一行带来源的日志。
pub fn emit_log_from(app: &tauri::AppHandle, source: &str, level: &str, line: impl Into<String>) {
    let _ = app.emit(
        EVT_LOG,
        serde_json::json!({ "line": line.into(), "level": level, "source": source }),
    );
}

/// 推送二维码图片（data URL）。
pub fn emit_qrcode(app: &tauri::AppHandle, data_url: impl Into<String>) {
    let _ = app.emit(EVT_QRCODE, serde_json::json!({ "data_url": data_url.into() }));
}
