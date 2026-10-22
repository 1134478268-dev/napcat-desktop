// 由 main.rs 拆分生成：配置读写命令
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::models::*;
use crate::services::*;
use crate::state::*;
use crate::utils::*;

/// 测试前后端连通性，返回 "pong from napcat-desktop"。
#[tauri::command]
pub fn ping() -> String {
    "pong from napcat-desktop".to_string()
}

#[tauri::command]
pub fn load_config(app: tauri::AppHandle, state: tauri::State<Arc<AppState>>) -> Result<AppConfig, String> {
    let p = config_path(&app)?;
    if !p.exists() { return Ok(AppConfig::default()); }
    let txt = fs::read_to_string(&p).map_err(|e| format!("读取配置失败: {e}"))?;
    let mut cfg: AppConfig = serde_json::from_str(&txt).map_err(|e| format!("配置 JSON 损坏: {e}"))?;
    // 自动修正插件目录：如果配置里的 plugins_dir 不存在，从 napcat_path 重新推断
    let configured = Path::new(&cfg.plugins_dir);
    if cfg.plugins_dir.is_empty() || !configured.is_dir() {
        let resolved = resolve_plugins_dir(&cfg.napcat_path);
        if !resolved.is_empty() { cfg.plugins_dir = resolved; }
    }
    *lock(&state.config) = cfg.clone();
    Ok(cfg)
}

/// 把整个配置对象覆盖写入 napcat-desktop.json。
/// 前端在账号增删改、设置页保存时调用。整对象覆盖，不是增量 patch。
/// 副作用：写文件 + 更新内存缓存。不重启子进程，不发事件。
#[tauri::command]
pub fn save_config(app: tauri::AppHandle, state: tauri::State<Arc<AppState>>, config: AppConfig) -> Result<(), String> {
    let p = config_path(&app)?;
    let txt = serde_json::to_string_pretty(&config).map_err(|e| format!("序列化失败: {e}"))?;
    fs::write(&p, txt).map_err(|e| format!("写入配置失败: {e}"))?;
    *lock(&state.config) = config;
    Ok(())
}
