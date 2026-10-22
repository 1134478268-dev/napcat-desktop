//! AI 接入服务：配置/人设/记忆的持久化 + 三种协议族的对话调用。
//!
//! 为什么用 curl 而不是引入 HTTP 客户端：
//!   项目当前依赖只有 tauri/serde/chrono，新增 reqwest 会带来一整套 TLS 依赖、
//!   显著拉长构建时间。Windows 10 1803+ 自带 curl.exe，够用。
//!
//! 密钥不进命令行：
//!   `Command::new("curl").arg("-H").arg("Authorization: Bearer sk-...")` 会让密钥
//!   出现在进程命令行里（同机任何进程 tasklist/QueryFullProcessImageName 都能读到）。
//!   这里改成把整个 curl 配置（含 header）从 **stdin** 喂给 `curl -K -`，
//!   请求体写临时文件（不含密钥），调用完立即删除。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tauri::Manager;

use crate::models::*;

/// AI 数据目录：<app_config_dir>/ai
pub fn ai_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法获取 app_config_dir: {e}"))?
        .join("ai");
    fs::create_dir_all(&dir).map_err(|e| format!("创建 AI 目录失败: {e}"))?;
    Ok(dir)
}

fn config_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(ai_dir(app)?.join("config.json"))
}

fn personas_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(ai_dir(app)?.join("personas.json"))
}

fn memories_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(ai_dir(app)?.join("memories.json"))
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &PathBuf) -> T {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return T::default(), // 文件不存在是正常情况（首次使用）
    };
    match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            // 静默回退默认值是最难排查的失败模式：界面显示"未配置"，
            // 但文件明明存在。把解析错误写到同目录，便于定位是哪个字段坏了。
            if let Some(dir) = path.parent() {
                let _ = fs::write(
                    dir.join("parse-error.log"),
                    format!("{}: {}\n", path.display(), e),
                );
            }
            T::default()
        }
    }
}

fn write_json<T: serde::Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败: {e}"))?;
    fs::write(path, text).map_err(|e| format!("写入 {} 失败: {e}", path.display()))
}

pub fn load_state(app: &tauri::AppHandle) -> Result<AiState, String> {
    let mut config = read_json::<AiConfig>(&config_file(app)?);
    // 落盘的是 DPAPI 密文；前端需要明文才能显示/编辑，所以在这里统一解密。
    // 旧版明文没有 dpapi: 前缀，decrypt 会原样返回，因此天然向后兼容。
    for p in &mut config.providers {
        p.api_key = crate::services::secret::decrypt(&p.api_key)?;
    }
    config.gateway.access_token = crate::services::secret::decrypt(&config.gateway.access_token)?;

    Ok(AiState {
        config,
        personas: read_json::<Vec<Persona>>(&personas_file(app)?),
        memories: read_json::<Vec<MemoryEntry>>(&memories_file(app)?),
    })
}

pub fn save_ai_config(app: &tauri::AppHandle, config: &AiConfig) -> Result<(), String> {
    // 写盘前加密：配置文件即使被拷走 / 同步到网盘 / 截图，也拿不到可用的 key。
    // is_encrypted 判断是防双重加密：万一某条路径绕过 load_state 直接拿到了密文，
    // 再加密一次会得到解不开的套娃。
    let mut cfg = config.clone();
    for p in &mut cfg.providers {
        if !crate::services::secret::is_encrypted(&p.api_key) {
            p.api_key = crate::services::secret::encrypt(&p.api_key)?;
        }
    }
    if !crate::services::secret::is_encrypted(&cfg.gateway.access_token) {
        cfg.gateway.access_token = crate::services::secret::encrypt(&cfg.gateway.access_token)?;
    }
    write_json(&config_file(app)?, &cfg)
}

pub fn save_ai_personas(app: &tauri::AppHandle, personas: &[Persona]) -> Result<(), String> {
    write_json(&personas_file(app)?, &personas.to_vec())
}

pub fn save_ai_memories(app: &tauri::AppHandle, memories: &[MemoryEntry]) -> Result<(), String> {
    write_json(&memories_file(app)?, &memories.to_vec())
}

// ===========================================================================
// Prompt 组装
// ===========================================================================

/// 拼 system prompt：人设 → 补充说明 → 记忆 → 工具清单（当前只做提示，不执行）。
fn build_system_prompt(state: &AiState) -> String {
    let cfg = &state.config;
    let mut parts: Vec<String> = Vec::new();

    if let Some(p) = state.personas.iter().find(|p| p.id == cfg.active_persona) {
        if !p.prompt.trim().is_empty() {
            parts.push(p.prompt.trim().to_string());
        }
    }
    if !cfg.system_extra.trim().is_empty() {
        parts.push(cfg.system_extra.trim().to_string());
    }

    if cfg.memory_enabled {
        let mut mems: Vec<&MemoryEntry> = state
            .memories
            .iter()
            .filter(|m| m.persona_id.is_empty() || m.persona_id == cfg.active_persona)
            .collect();
        // 只取最近的 N 条，避免把上下文塞爆
        let limit = cfg.memory_limit.max(1) as usize;
        if mems.len() > limit {
            mems = mems.split_off(mems.len() - limit);
        }
        if !mems.is_empty() {
            let lines: Vec<String> = mems.iter().map(|m| format!("- {}", m.content)).collect();
            parts.push(format!("你记得以下关于用户的事实：\n{}", lines.join("\n")));
        }
    }

    if cfg.tool_calling_enabled {
        // 预留：真正接工具执行前，只把可用能力告诉模型，让它知道能请求什么。
        parts.push(
            "你可以请求调用以下工具，但当前版本暂不执行，请先用自然语言回答：\n\
             - send_message(target, text)：向指定 QQ 会话发消息\n\
             - list_groups()：列出机器人所在群\n\
             - get_group_members(group_id)：查群成员"
                .to_string(),
        );
    }

    parts.join("\n\n")
}

// ===========================================================================
// 各协议族的请求体
// ===========================================================================

fn chat_url(provider: &AiProvider) -> String {
    let base = provider.api_base.trim_end_matches('/');
    match provider.kind.as_str() {
        "anthropic" if !base.ends_with("/messages") => format!("{base}/messages"),
        "gemini" => {
            // Gemini 的模型路径是 base/models/<model>:generateContent
            if base.contains(":generateContent") {
                base.to_string()
            } else {
                format!("{base}/models/{}:generateContent", provider.model)
            }
        }
        _ if base.ends_with("/chat/completions") => base.to_string(),
        _ => format!("{base}/chat/completions"),
    }
}

fn build_body(kind: &str, provider: &AiProvider, system: &str, messages: &[ChatMessage], cfg: &AiConfig) -> Value {
    match kind {
        "anthropic" => json!({
            "model": provider.model,
            "system": system,
            "max_tokens": cfg.max_tokens,
            "temperature": cfg.temperature,
            "messages": messages.iter()
                .filter(|m| m.role != "system")
                .map(|m| json!({ "role": m.role, "content": m.content }))
                .collect::<Vec<_>>(),
        }),
        "gemini" => {
            let contents: Vec<Value> = messages
                .iter()
                .filter(|m| m.role != "system")
                .map(|m| json!({
                    "role": if m.role == "assistant" { "model" } else { "user" },
                    "parts": [{ "text": m.content }]
                }))
                .collect();
            json!({
                "systemInstruction": { "parts": [{ "text": system }] },
                "contents": contents,
                "generationConfig": {
                    "temperature": cfg.temperature,
                    "maxOutputTokens": cfg.max_tokens
                }
            })
        }
        _ => {
            let mut msgs: Vec<Value> = Vec::new();
            if !system.is_empty() {
                msgs.push(json!({ "role": "system", "content": system }));
            }
            for m in messages {
                msgs.push(json!({ "role": m.role, "content": m.content }));
            }
            json!({
                "model": provider.model,
                "messages": msgs,
                "temperature": cfg.temperature,
                "max_tokens": cfg.max_tokens,
            })
        }
    }
}

fn auth_headers(kind: &str, api_key: &str) -> Vec<(String, String)> {
    match kind {
        "anthropic" => vec![
            ("x-api-key".to_string(), api_key.to_string()),
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
        ],
        "gemini" => vec![("x-goog-api-key".to_string(), api_key.to_string())],
        _ => vec![("Authorization".to_string(), format!("Bearer {api_key}"))],
    }
}

// ===========================================================================
// HTTP（curl -K -，密钥走 stdin）
// ===========================================================================

fn post_json(url: &str, body: &Value, headers: &[(String, String)]) -> Result<String, String> {
    let body_path = std::env::temp_dir().join(format!(
        "napcat-ai-{}-{}.json",
        std::process::id(),
        chrono::Local::now().timestamp_millis()
    ));
    fs::write(&body_path, body.to_string()).map_err(|e| format!("写请求体失败: {e}"))?;

    // curl 配置文件写在 stdin：密钥只存在于管道里
    let mut cfg = String::new();
    cfg.push_str(&format!("url = \"{}\"\n", escape_curl(url)));
    cfg.push_str("silent\nshow-error\nmax-time = 180\n");
    cfg.push_str("header = \"Content-Type: application/json\"\n");
    for (k, v) in headers {
        cfg.push_str(&format!(
            "header = \"{}: {}\"\n",
            escape_curl(k),
            escape_curl(v)
        ));
    }
    cfg.push_str(&format!(
        "data-binary = \"@{}\"\n",
        escape_curl(&body_path.to_string_lossy())
    ));
    cfg.push_str("write-out = \"\\n%{http_code}\"\n");

    let mut child = Command::new("curl")
        .arg("-K")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 curl（系统是否自带 curl.exe？）: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(cfg.as_bytes())
            .map_err(|e| format!("向 curl 写入配置失败: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("等待 curl 失败: {e}"))?;
    let _ = fs::remove_file(&body_path);

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // 末尾那行是 http_code
    let (raw, code) = match stdout.rsplit_once('\n') {
        Some((body_part, code_part)) => (body_part.to_string(), code_part.trim().to_string()),
        None => (stdout.clone(), String::new()),
    };

    if !out.status.success() {
        let msg = if stderr.trim().is_empty() { raw.trim().to_string() } else { stderr.trim().to_string() };
        return Err(format!("请求失败（curl 退出码 {}）: {msg}", out.status.code().unwrap_or(-1)));
    }
    if let Ok(n) = code.parse::<u16>() {
        if n >= 400 {
            return Err(format!("上游返回 HTTP {n}: {}", truncate(&raw, 500)));
        }
    }
    Ok(raw)
}

fn escape_curl(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

// ===========================================================================
// 响应解析
// ===========================================================================

fn parse_reply(kind: &str, raw: &str) -> Result<AiChatResult, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| format!("上游返回不是合法 JSON: {e}"))?;
    let mut r = AiChatResult::default();

    match kind {
        "anthropic" => {
            let text = v["content"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter(|c| c["type"] == "text")
                        .filter_map(|c| c["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            r.content = text;
            r.model = v["model"].as_str().unwrap_or_default().to_string();
            r.prompt_tokens = v["usage"]["input_tokens"].as_u64().map(|x| x as u32);
            r.completion_tokens = v["usage"]["output_tokens"].as_u64().map(|x| x as u32);
        }
        "gemini" => {
            r.content = v["candidates"][0]["content"]["parts"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            r.model = v["modelVersion"].as_str().unwrap_or_default().to_string();
            r.prompt_tokens = v["usageMetadata"]["promptTokenCount"].as_u64().map(|x| x as u32);
            r.completion_tokens = v["usageMetadata"]["candidatesTokenCount"].as_u64().map(|x| x as u32);
        }
        _ => {
            let msg = &v["choices"][0]["message"];
            r.content = msg["content"].as_str().unwrap_or_default().to_string();
            r.model = v["model"].as_str().unwrap_or_default().to_string();
            r.prompt_tokens = v["usage"]["prompt_tokens"].as_u64().map(|x| x as u32);
            r.completion_tokens = v["usage"]["completion_tokens"].as_u64().map(|x| x as u32);
            // 预留：模型要求调工具时，透传给前端（当前后端不执行）
            if let Some(calls) = msg["tool_calls"].as_array() {
                r.tool_calls = calls.clone();
            }
        }
    }

    if r.content.trim().is_empty() && r.tool_calls.is_empty() {
        return Err(format!("上游没有返回内容: {}", truncate(raw, 300)));
    }
    Ok(r)
}

/// 发起一次对话。
pub fn chat(app: &tauri::AppHandle, provider_id: &str, messages: &[ChatMessage]) -> Result<AiChatResult, String> {
    let state = load_state(app)?;
    let cfg = &state.config;

    if !cfg.enabled {
        return Err("AI 接入已关闭：请先在 AI 页打开总开关".to_string());
    }

    let provider = cfg
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .or_else(|| cfg.providers.iter().find(|p| p.enabled))
        .ok_or("尚未配置任何 AI 服务，请先在 AI 页添加")?;

    if provider.api_base.trim().is_empty() {
        return Err(format!("「{}」的 API 地址为空", provider.name));
    }
    if provider.model.trim().is_empty() {
        return Err(format!("「{}」的模型名为空", provider.name));
    }
    if provider.api_key.trim().is_empty() {
        return Err(format!("「{}」的 API Key 为空", provider.name));
    }

    let system = build_system_prompt(&state);
    let url = chat_url(provider);
    let body = build_body(&provider.kind, provider, &system, messages, cfg);
    let headers = auth_headers(&provider.kind, provider.api_key.trim());
    let raw = post_json(&url, &body, &headers)?;
    parse_reply(&provider.kind, &raw)
}
