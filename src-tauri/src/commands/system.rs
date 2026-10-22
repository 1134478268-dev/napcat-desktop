// 由 main.rs 拆分生成：系统交互命令（打开路径 / 导出日志）
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::Path;
use std::process::Command;
use tauri::Manager;


/// 在系统默认程序里打开一个外部路径或 URL。
/// path 可以是目录绝对路径（资源管理器打开）或 http(s):// URL（默认浏览器打开）。
///
/// === 为什么必须拒绝 shell 元字符 ===
/// 这里最终仍是 `cmd /C start "" <path>`：cmd.exe 在解析阶段就会处理 `& | < > ^`
/// （分隔命令、重定向、转义），即使 start 自己不解释它们，路径里一个 `&` 就能把
/// 一条打开命令拆成两条。所以打开前先拒绝含这些字符的 path，思路与 launch_child
/// 里的 launcher 校验一致。代价：带查询参数（含 &）的 URL 会被一并拒绝——这是
/// 刻意的取舍，宁可让用户少打开一个链接，也不留一条可以拼命令的 IPC 通道。
#[tauri::command]
pub fn open_external(path: String) -> Result<(), String> {
    let p = path.trim().to_string();
    if p.is_empty() { return Err("路径为空".to_string()); }
    // 除了 cmd 的命令分隔/重定向/转义符，还必须拒绝**双引号**：
    // Rust 传给 cmd.exe 时会把内部 `"` 转义成 `\"`，而 cmd.exe 不认 `\"`、
    // 只按裸 `"` 切换引号状态 —— 于是"空格 + 一个双引号"就能提前闭合引号，
    // 把后面的内容变成新的命令段（独立审计用 rustc 探针实测：含 `"` 与 `&` 的
    // URL 确实能注入执行）。同理拒绝 `%`（cmd 会做 %VAR% 展开）、`!`（延迟展开）、
    // 反引号与换行（换行在 cmd 里同样是语句分隔符）。
    if p.chars().any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '"' | '%' | '!' | '`' | '\n' | '\r')) {
        return Err("路径包含 shell 元字符或引号，已拒绝打开".to_string());
    }
    // URL：交给默认浏览器
    if p.starts_with("http://") || p.starts_with("https://") {
        Command::new("cmd").args(["/C", "start", "", &p]).spawn()
            .map_err(|e| format!("打开 URL 失败: {e}"))?;
        return Ok(());
    }
    // 目录/文件：必须存在
    if !Path::new(&p).exists() { return Err(format!("路径不存在: {p}")); }
    Command::new("cmd").args(["/C", "start", "", &p]).spawn()
        .map_err(|e| format!("打开路径失败: {e}"))?;
    Ok(())
}

/// 单次导出日志的字节上限（8 MiB）。
/// 前端理论上可以传任意大的字符串过来，不加限制就是"一次 IPC 写满磁盘"，
/// 所以设硬上限；8 MiB 已足够几万行日志，超过让用户先清空或分批导出。
pub const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;

/// 把前端传来的日志文本写入磁盘文件，返回写入的绝对路径。
/// 文件放在 app_config_dir/logs-<timestamp>.txt。
#[tauri::command]
pub fn export_logs(app: tauri::AppHandle, content: String) -> Result<String, String> {
    if content.len() > MAX_EXPORT_BYTES {
        return Err(format!("导出内容过大（{} 字节，上限 {} 字节），请先清空或分批导出",
            content.len(), MAX_EXPORT_BYTES));
    }
    let dir = app.path().app_config_dir().map_err(|e| format!("无法获取配置目录: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    // 文件名时间戳用 chrono::Local（本地时区）。原来手写的日历换算基于 UTC 秒，
    // 在东八区生成的文件名会比真实时间早 8 小时，用户按时间找日志会对不上；
    // chrono 本来就是依赖（error.log 在用），不必自己算日历。
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let file = dir.join(format!("logs-{ts}.txt"));
    fs::write(&file, content).map_err(|e| format!("写入失败: {e}"))?;
    Ok(file.to_string_lossy().to_string())
}

// ===========================================================================
// 入口
// ===========================================================================
