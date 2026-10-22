// 由 main.rs 拆分生成：NoneBot 机器人进程命令
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use crate::services::*;
use crate::state::*;
use crate::utils::*;

/// 在插件目录下找 bot.py 并启动 NoneBot 机器人。
///
/// 为什么要单独管：用户下载的"NapCat 插件包"实际是 NoneBot2 项目
/// （bot.py + plugins/ 子目录），它不会被 NapCat 自动加载，
/// 必须自己 spawn python bot.py，通过 OneBot 协议连到 NapCat。
#[tauri::command]
pub fn start_bot(app: tauri::AppHandle, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let st = state.inner().clone();
    if lock(&st.bot_pid).is_some() { return Err("机器人已在运行".to_string()); }
    let plugins_dir = {
        let cfg = lock(&st.config);
        resolve_plugins_dir(&cfg.napcat_path)
    };
    // 与 install_plugin 同样的防空：配置未加载时目录是空串/相对路径，
    // read_dir 会去扫"进程当前目录"，可能从一个意外位置拉起 bot.py。
    let plugins_dir = ensure_plugins_dir(&plugins_dir)?;

    // 找 bot.py：优先 plugins/<某子目录>/bot.py
    let mut bot_py: Option<PathBuf> = None;
    if let Ok(rd) = fs::read_dir(&plugins_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let cand = p.join("bot.py");
                if cand.exists() { bot_py = Some(cand); break; }
            }
        }
    }
    let bot_py = bot_py.ok_or_else(|| format!("在 {} 下未找到 bot.py（NoneBot 入口）", plugins_dir.display()))?;
    let work_dir = bot_py
        .parent()
        .ok_or_else(|| format!("bot.py 没有父目录: {}", bot_py.display()))?
        .to_path_buf();

    let mut cmd = Command::new("python");
    cmd.arg(&bot_py).current_dir(&work_dir).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("启动 python bot.py 失败: {e}"))?;
    let pid = child.id();
    *lock(&st.bot_pid) = Some(pid);

    // stdout/stderr 走共用的转发（脱敏 + 分级 + error.log），只额外加 "[bot] " 前缀，
    // 让用户在同一个日志视图里能分辨哪行来自 NapCat、哪行来自 NoneBot。
    attach_log_forwarders_with_source(&mut child, &app, "", SRC_BOT);
    // 先 emit 启动日志（app 还没被 move）
    emit_log_from(&app, SRC_BOT, "ok", format!("[bot] 已启动 NoneBot 机器人 (PID={pid})，目录: {}", work_dir.display()));
    // 放后台 watcher：bot 退出时清空 pid，前端状态会跟着变
    std::thread::spawn(move || {
        let _ = child.wait();
        *lock(&st.bot_pid) = None;
        emit_log_from(&app, SRC_BOT, "warn", format!("[bot] 机器人进程已退出 (PID={pid})"));
    });
    Ok(())
}

/// 停止 NoneBot 机器人进程。
#[tauri::command]
pub fn stop_bot(app: tauri::AppHandle, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let st = state.inner().clone();
    let pid = *lock(&st.bot_pid);
    if let Some(p) = pid {
        std::thread::spawn(move || {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &p.to_string()])
                .status();
            std::thread::sleep(Duration::from_millis(400));
            *lock(&st.bot_pid) = None;
            emit_log_from(&app, SRC_BOT, "info", format!("[bot] 已停止机器人 (PID={p})"));
        });
    }
    Ok(())
}

/// 强制清理机器人进程（不做任何"是否在运行"的前置判断）。
///
/// 为什么需要它，以及为什么不能只靠 stop_bot：
///   · stop_bot 只在 bot_pid 有值时才动手。而用户从托盘「退出」时走的是
///     app.exit(0)，过去根本不经过 stop_bot —— 于是 python bot.py 变成孤儿进程，
///     继续占着 NoneBot 的 8080 端口。下次启动就会撞上
///     `[Errno 10048] error while attempting to bind on address ('127.0.0.1', 8080)`，
///     新进程随即退出，用户看到的就是"机器人一启动就崩"。
///   · NCD 自己被任务管理器强杀时，watcher 线程来不及把 bot_pid 清成 None，
///     也会留下占端口的孤儿。
///
/// 所以这里做两件事：先按已知 pid 杀进程树，再按命令行特征兜底扫一遍残留。
pub fn force_kill_bot(st: &Arc<AppState>) -> Vec<u32> {
    let mut killed: Vec<u32> = Vec::new();

    // 1) 已知 pid：/T 连子进程树一起杀
    let known = lock(&st.bot_pid).take();
    if let Some(p) = known {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &p.to_string()])
            .status();
        killed.push(p);
    }

    // 2) 兜底：把命令行里含 bot.py 的 python 进程全找出来。
    //    bot_pid 丢失（强杀 / 崩溃）时，这是唯一还能定位到孤儿的办法。
    //    用 Win32_Process 而不是 wmic —— wmic 在新版 Windows 已被移除。
    let ps = "Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" | \
              Where-Object { $_.CommandLine -like '*bot.py*' } | \
              ForEach-Object { $_.ProcessId }";
    if let Ok(out) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Ok(p) = line.trim().parse::<u32>() {
                if !killed.contains(&p) {
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &p.to_string()])
                        .status();
                    killed.push(p);
                }
            }
        }
    }

    // 3) 释放 8080：NoneBot 默认端口。只有确认占用者是本次清掉的进程时才不动手，
    //    否则仅记录一条提示由调用方展示，避免误杀无关程序。
    *lock(&st.bot_pid) = None;
    killed
}

/// 前端「强制结束机器人」按钮：清掉已知 pid + 扫掉命令行残留的 bot.py 进程。
#[tauri::command]
pub fn kill_bot(app: tauri::AppHandle, state: tauri::State<Arc<AppState>>) -> Result<String, String> {
    let st = state.inner().clone();
    let killed = force_kill_bot(&st);
    std::thread::sleep(Duration::from_millis(300));

    let msg = if killed.is_empty() {
        "未发现残留的机器人进程".to_string()
    } else {
        format!("已强制结束 {} 个机器人进程: {:?}", killed.len(), killed)
    };
    emit_log_from(&app, SRC_BOT, "warn", format!("[bot] {msg}"));
    Ok(msg)
}

/// 查询 bot 是否在运行。
#[tauri::command]
pub fn get_bot_status(state: tauri::State<Arc<AppState>>) -> bool {
    lock(&state.bot_pid).is_some()
}
