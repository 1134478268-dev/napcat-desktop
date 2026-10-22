// 子进程日志转发与落盘
// 该模块的职责见 docs/ARCHITECTURE.md。
//
// 日志按「来源」分流：source ∈ napcat / ai / bot。
//   · 前端：payload 里带 source，日志页可按来源筛选
//   · 落盘：napcat 与 bot 写 error.log，ai 单独写 ai-error.log
// 为什么分流：排查时两者的噪声完全不同 —— NapCat 的报错多是登录/协议/插件，
// AI 的多是 key、限流、网络超时。混在一个文件里只能靠前缀肉眼过滤。

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::Child;

use crate::services::*;
use crate::utils::*;

/// 把一条日志追加到落盘文件（仅当 level 是 err 或 warn）。默认来源 napcat。
pub fn write_error_log(app: &tauri::AppHandle, level: &str, line: &str) {
    write_error_log_from(app, SRC_NAPCAT, level, line);
}

/// 按来源写不同的日志文件。
/// 写成一行：[YYYY-MM-DD HH:MM:SS] [LEVEL] [source] message
/// 失败不影响主流程（best-effort，日志写不进去就丢弃）。
pub fn write_error_log_from(app: &tauri::AppHandle, source: &str, level: &str, line: &str) {
    if level != "err" && level != "warn" {
        return;
    }
    let path = if source == SRC_AI {
        ai_error_log_path(app)
    } else {
        error_log_path(app)
    };
    let Ok(path) = path else { return };

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let entry = format!("[{now}] [{level:>4}] [{source}] {line}\n");
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

/// 把子进程 stdout/stderr 接到前端（脱敏、分级、二维码、落盘）。默认来源 napcat。
///
/// 这套逻辑以前在三处各写了一份（launcher / bootmain node / NoneBot bot.py），
/// 行为还不一致：只有 bootmain 那份认二维码、只有 stderr 那份无条件标 err、
/// bot 那份分级更粗糙。统一到这里后，新增一种启动形态只需接一次。
/// 顺带修掉一个隐患：老 bootmain 分支把 stderr 设成 piped() 却没人读，
/// 64KB 管道缓冲区写满后子进程会阻塞在 stderr 上"假死"。
pub fn attach_log_forwarders(child: &mut Child, app: &tauri::AppHandle) {
    attach_log_forwarders_with_source(child, app, "", SRC_NAPCAT);
}

/// 转发实现体。`source` 决定事件 payload 的来源字段与落盘文件。
pub fn attach_log_forwarders_with_source(
    child: &mut Child,
    app: &tauri::AppHandle,
    prefix: &str,
    source: &str,
) {
    // 把 stdout 抽出来交给一个常驻线程逐行 emit。
    // 注意 take() 必须在 spawn 之后、把 child 存进 state 之前做，否则就拿不到了。
    if let Some(out) = child.stdout.take() {
        let app2 = app.clone();
        let prefix = prefix.to_string();
        let source = source.to_string();
        std::thread::spawn(move || {
            // map_while 而不是 flatten：line() 反复报错时 flatten 会空转到底（死循环），
            // map_while 在第一个 Err 就结束循环。管道关闭后线程自然退出。
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let lvl = classify_log(&line);
                // 脱敏：把 token=xxx 之类的凭据换成 ***，避免推到前端日志视图
                let text = format!("{prefix}{}", redact(&line));
                emit_log_from(&app2, &source, lvl, text.as_str());
                write_error_log_from(&app2, &source, lvl, &text);
                // 二维码：所有启动形态都可能打印二维码路径，读到就转 base64 emit 给
                // 前端弹窗显示（不落盘，避免在磁盘留下与登录凭据相关的新文件）。
                if let Some(png) = find_qrcode_path(&line) {
                    if let Ok(bytes) = fs::read(&png) {
                        let b64 = base64_encode(&bytes);
                        emit_qrcode(&app2, format!("data:image/png;base64,{b64}"));
                    }
                }
            }
        });
    }
    // stderr 一律标 err，方便前端染红。
    if let Some(err) = child.stderr.take() {
        let app2 = app.clone();
        let prefix = prefix.to_string();
        let source = source.to_string();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let text = format!("{prefix}{}", redact(&line));
                emit_log_from(&app2, &source, "err", text.as_str());
                write_error_log_from(&app2, &source, "err", &text);
            }
        });
    }
}
