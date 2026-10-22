// 系统交互命令（打开路径 / 导出日志）
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::Path;
use tauri::Manager;

/// 用系统默认程序打开一个外部路径或 URL。
///
/// === 为什么不再用 `cmd /C start` ===
/// 旧实现是 `Command::new("cmd").args(["/C","start","",path])`，等于把路径交回
/// cmd.exe 做第二遍解析：`& | < > ^ ( )` 都会被它解释成命令分隔/重定向/分组。
/// Rust 把内部 `"` 转义成 `\"`，而 cmd 只认裸 `"`，于是"空格 + 一个双引号"
/// 就能提前闭合引号追加命令（独立审计用 rustc 探针实测过）。
/// 现在改用 `ShellExecuteW`：系统 API 直接接收路径字符串，**没有 shell 解析层**，
/// 从根上消除这条注入通道。
///
/// 元字符黑名单保留作为纵深防御：即使将来有人改回 cmd，也不会立刻变成洞。
#[tauri::command]
pub fn open_external(path: String) -> Result<(), String> {
    let p = path.trim().to_string();
    if p.is_empty() {
        return Err("路径为空".to_string());
    }
    // 纵深防御：拒绝 cmd 的命令分隔/重定向/展开符与引号、换行。
    // 对 ShellExecuteW 本身这些字符是安全的，但保留校验可以防"以后有人改回 cmd"。
    if p.chars()
        .any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '"' | '%' | '!' | '`' | '\n' | '\r'))
    {
        return Err("路径包含 shell 元字符或引号，已拒绝打开".to_string());
    }

    let is_url = p.starts_with("http://") || p.starts_with("https://");
    // 非 URL 必须存在，避免把任意字符串交给系统去猜关联程序
    if !is_url && !Path::new(&p).exists() {
        return Err(format!("路径不存在: {p}"));
    }

    shell_execute(&p).map_err(|e| {
        if is_url {
            format!("打开 URL 失败: {e}")
        } else {
            format!("打开路径失败: {e}")
        }
    })
}

/// 调 ShellExecuteW 打开目标（URL 交给默认浏览器，路径交给文件关联）。
#[cfg(windows)]
fn shell_execute(target: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let op = wide("open");
    let file = wide(target);

    // SAFETY: 两个宽字符串都以 NUL 结尾且在本调用期间有效；
    // 其余参数为 null 表示使用默认行为。
    let ret = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW 的返回值 > 32 才算成功（这是 Win32 的古老约定，不是错误码）
    if (ret as isize) <= 32 {
        Err(format!("ShellExecute 返回 {}", ret as isize))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn shell_execute(_target: &str) -> Result<(), String> {
    Err("当前平台未实现".to_string())
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
        return Err(format!(
            "导出内容过大（{} 字节，上限 {} 字节），请先清空或分批导出",
            content.len(),
            MAX_EXPORT_BYTES
        ));
    }
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法获取配置目录: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    // 文件名时间戳用 chrono::Local（本地时区）。原来手写的日历换算基于 UTC 秒，
    // 在东八区生成的文件名会比真实时间早 8 小时，用户按时间找日志会对不上；
    // chrono 本来就是依赖（error.log 在用），不必自己算日历。
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let file = dir.join(format!("logs-{ts}.txt"));
    fs::write(&file, content).map_err(|e| format!("写入失败: {e}"))?;
    Ok(file.to_string_lossy().to_string())
}
