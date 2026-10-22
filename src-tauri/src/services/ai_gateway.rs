//! AI 自动回复网关的进程托管。
//!
//! 网关本体是 `assets/ai-gateway.mjs`（编译期 include_str! 进 exe），
//! 运行时写到 `<ai 目录>/gateway.mjs` 再 spawn node。
//!
//! 为什么每次启动都覆盖写脚本：升级桌面端后，若沿用上次落盘的旧脚本，
//! 会出现「面板是新版、网关还是旧版」的诡异状态，排查成本极高。

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use crate::models::*;
use crate::services::ai_service;
use crate::services::logging::attach_log_forwarders_with_source;
use crate::services::events::{emit_log_from, SRC_AI};
use crate::state::AppState;
use crate::utils::lock;

const GATEWAY_JS: &str = include_str!("../../assets/ai-gateway.mjs");

fn gateway_script(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = ai_service::ai_dir(app)?;
    let p = dir.join("gateway.mjs");
    fs::write(&p, GATEWAY_JS).map_err(|e| format!("写入网关脚本失败: {e}"))?;
    Ok(p)
}

/// 启动网关。会校验「自动回复已启用 + AI 服务配置完整」，缺失时直接报错，
/// 而不是让 Node 进程起来两秒再崩（那样用户只能看到一堆日志）。
pub fn start(app: tauri::AppHandle, state: Arc<AppState>) -> Result<AiGatewayStatus, String> {
    if lock(&state.gateway_pid).is_some() {
        return Err("AI 网关已在运行".to_string());
    }

    let st = ai_service::load_state(&app)?;
    if !st.config.enabled {
        return Err("AI 接入已关闭：请先在 AI 页打开总开关".to_string());
    }
    let gateway = &st.config.gateway;
    if !gateway.enabled {
        return Err("自动回复未启用：请先在 AI 页打开「启用自动回复」".to_string());
    }

    let provider = st
        .config
        .providers
        .iter()
        .find(|p| p.id == st.config.active_provider)
        .or_else(|| st.config.providers.first())
        .ok_or("尚未配置 AI 服务")?;
    if provider.api_key.trim().is_empty() || provider.model.trim().is_empty() || provider.api_base.trim().is_empty() {
        return Err(format!("「{}」的 API Key / 模型 / 地址不完整", provider.name));
    }

    let script = gateway_script(&app)?;
    let dir = ai_service::ai_dir(&app)?;

    let mut cmd = Command::new("node");
    cmd.arg(&script)
        .env("NAPCAT_AI_DIR", &dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 node 失败（需要 Node.js 在 PATH 中）: {e}"))?;

    attach_log_forwarders_with_source(&mut child, &app, "", SRC_AI);
    let pid = child.id();
    *lock(&state.gateway_pid) = Some(pid);

    let app2 = app.clone();
    let st2 = state.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        *lock(&st2.gateway_pid) = None;
        emit_log_from(&app2, SRC_AI, "warn", "网关进程已退出");
    });

    emit_log_from(&app, SRC_AI, "ok", format!("自动回复网关已启动 PID={pid}"));
    Ok(AiGatewayStatus {
        running: true,
        pid: Some(pid),
        message: format!("已启动 PID={pid}"),
    })
}

/// 停止网关（结束整棵进程树）。
pub fn stop(state: &Arc<AppState>) -> Result<(), String> {
    let pid = *lock(&state.gateway_pid);
    let Some(p) = pid else {
        return Err("AI 网关未在运行".to_string());
    };
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &p.to_string()])
        .status();
    *lock(&state.gateway_pid) = None;
    Ok(())
}

pub fn status(state: &Arc<AppState>) -> AiGatewayStatus {
    let pid = *lock(&state.gateway_pid);
    AiGatewayStatus {
        running: pid.is_some(),
        pid,
        message: String::new(),
    }
}
