//! AI 相关命令：只做参数转发，业务在 `services::ai_service` / `services::ai_gateway`。

use std::sync::Arc;

use crate::models::*;
use crate::services::ai_gateway;
use crate::services::ai_service;
use crate::state::AppState;

/// 一次取回 AI 的配置 / 人设 / 记忆。
///
/// 为什么合并成一个命令：三者体量都很小（几 KB），分开取只会让前端多两次往返，
/// 还容易出现"配置更新了、人设没更新"的中间态。整存整取更简单。
#[tauri::command]
pub fn ai_load_state(app: tauri::AppHandle) -> Result<AiState, String> {
    ai_service::load_state(&app)
}

/// 覆盖写 AI 配置（providers / 当前人设 / 记忆开关等）。
#[tauri::command]
pub fn ai_save_config(app: tauri::AppHandle, config: AiConfig) -> Result<(), String> {
    ai_service::save_ai_config(&app, &config)
}

/// 覆盖写人设列表。
#[tauri::command]
pub fn ai_save_personas(app: tauri::AppHandle, personas: Vec<Persona>) -> Result<(), String> {
    ai_service::save_ai_personas(&app, &personas)
}

/// 覆盖写记忆列表。
#[tauri::command]
pub fn ai_save_memories(app: tauri::AppHandle, memories: Vec<MemoryEntry>) -> Result<(), String> {
    ai_service::save_ai_memories(&app, &memories)
}

/// 发起一次对话。
///
/// 网络请求是阻塞的（curl 子进程），必须放到 spawn_blocking 里，
/// 否则会堵住 Tauri 的异步运行时线程池，表现为整个界面卡住。
#[tauri::command]
pub async fn ai_chat(
    app: tauri::AppHandle,
    provider_id: String,
    messages: Vec<ChatMessage>,
) -> Result<AiChatResult, String> {
    tauri::async_runtime::spawn_blocking(move || ai_service::chat(&app, &provider_id, &messages))
        .await
        .map_err(|e| format!("AI 任务执行失败: {e}"))?
}

// ===========================================================================
// 自动回复网关
// ===========================================================================

/// 启动网关（Node 子进程，连 NapCat 的 WS 自动回消息）。
#[tauri::command]
pub fn ai_gateway_start(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
) -> Result<AiGatewayStatus, String> {
    match ai_gateway::start(app.clone(), state.inner().clone()) {
        Ok(v) => Ok(v),
        Err(e) => {
            // 失败原因落盘：这个错误只会在 UI 上闪一下 toast，事后无从排查
            crate::services::logging::write_error_log(&app, "warn", &format!("[ai] 网关启动失败: {e}"));
            Err(e)
        }
    }
}

/// 停止网关。
#[tauri::command]
pub fn ai_gateway_stop(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    ai_gateway::stop(state.inner())
}

/// 查询网关状态。
#[tauri::command]
pub fn ai_gateway_status(state: tauri::State<Arc<AppState>>) -> AiGatewayStatus {
    ai_gateway::status(state.inner())
}
