// 由 main.rs 拆分生成：崩溃自动拉起与状态广播
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;

use crate::services::*;
use crate::state::*;
use crate::utils::*;

/// 统一构造并发送 napcat://status 事件。
/// 为什么集中成一个函数：payload 里的 qq 是前端判断"哪个账号在运行"的关键字，
/// 早期实现里自动重启那几条出口漏了 qq，导致前端账号卡片状态漂移。
/// 集中之后所有出口字段完全一致，新增出口也不会再漏键。
pub fn emit_status(
    app: &tauri::AppHandle, qq: &str, running: bool, pid: Option<u32>,
    last_exit_code: Option<i32>, restart_count: u32, message: String,
) {
    let _ = app.emit(EVT_STATUS, serde_json::json!({
        "running": running,
        "pid": pid,
        "qq": qq,
        "last_exit_code": last_exit_code,
        "restart_count": restart_count,
        "message": message,
    }));
}

/// 标记"有进程在运行"，同时把 pid 写进操作句柄。
/// running 与 current_pid 必须成对维护，否则又会出现"UI 说在跑、操作拿不到 pid"。
pub fn mark_running(state: &AppState, pid: u32) {
    *lock(&state.running) = true;
    *lock(&state.current_pid) = Some(pid);
}

/// 清掉运行状态。`only_if_pid`：只有 current_pid 仍等于它时才清，用来避免旧 watcher
/// 线程在新一轮启动之后才走到收尾逻辑，把新一轮的运行状态误清成"未运行"；
/// 传 None 表示无条件清（用于确认已经没有任何子进程可管的情况）。
pub fn mark_stopped(state: &AppState, only_if_pid: Option<u32>) {
    // 先取快照再逐个加锁，避免同时持有两把锁（加锁顺序不一致会死锁）。
    let cur = *lock(&state.current_pid);
    if let Some(p) = only_if_pid {
        if cur != Some(p) { return; }
    }
    *lock(&state.running) = false;
    *lock(&state.current_pid) = None;
}

/// 启动 watcher 线程：阻塞 wait() 子进程退出，必要时按指数退避重新拉起。
///
/// 退避策略：1s -> 2s -> 4s -> 8s -> 16s，封顶 16s。
///   为什么指数：刚崩溃时 NapCat 可能只是启动阶段偶发失败，立刻重启会再踩同样的雷，
///   退避让系统/网络/文件锁有时间恢复；封顶 16s 是因为再长用户会以为面板卡死。
///   最多 max_restarts 次（默认 5）后放弃，避免配置错误导致死循环空转 CPU。
///   正常退出（code==0）不重启；用户主动 stop_napcat 会把 auto_restart 置 false。
/// 每条出口都必须同时更新 running / current_pid，否则前端状态会僵在"运行中"。
pub fn spawn_watcher(app: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        // 每轮先取参数快照：qq 用于所有 status 事件（前端靠它判断哪个账号在跑），
        // kind/cwd 用于必要时重启。取完立刻放锁，绝不在持锁期间 spawn/sleep/wait。
        let snap = {
            let wg = lock(&state.watcher);
            wg.as_ref().map(|w| (w.kind.clone(), w.qq.clone(), w.cwd.clone(),
                w.auto_restart, w.restart_count, w.max_restarts))
        };
        let Some((kind, qq, cwd, auto_restart, cur_r, max_r)) = snap else {
            // 控制块被清掉（正常流程不会发生），保守退出并复位状态。
            mark_stopped(&state, None);
            break;
        };
        // 把 child take 出来独占才能 wait()。take 之后 running 与 current_pid 才是
        // 唯一的运行判据，state.child 恒为 None。
        let waited = lock(&state.child).take();
        let Some(mut child) = waited else {
            // 没有句柄可等：本轮已被其它出口收尾（stop/kill 清理过），复位后退出。
            mark_stopped(&state, None);
            emit_status(&app, &qq, false, None, None, cur_r, "未运行".to_string());
            break;
        };
        let pid = child.id();
        // wait() 可能阻塞几小时，但在独立线程里，不影响 Tauri 主线程。
        // 这里不能 unwrap：wait 失败会 panic 掉整个 watcher 线程，
        // 之后 running 永远停在 true、所有按钮失效，是最难排查的僵尸状态。
        let code = match child.wait() {
            Ok(status) => status.code().unwrap_or(-1),
            Err(e) => {
                emit_log(&app, "err", format!("等待子进程退出失败: {e}"));
                -1
            }
        };

        if code != 0 && auto_restart && cur_r < max_r {
            // 指数退避：restart_count=0 等 1s，1 等 2s ... 最多 16s（1<<4）
            let backoff = 1u64 << cur_r.min(4);
            emit_status(&app, &qq, false, Some(pid), Some(code), cur_r,
                format!("进程异常退出 code={code}，{backoff}s 后自动重启"));
            std::thread::sleep(Duration::from_secs(backoff));
            // 退避期间用户可能点了"停止"，必须复查一次，否则"停止"之后还是会被拉起来。
            let still_auto = lock(&state.watcher).as_ref().map(|w| w.auto_restart).unwrap_or(false);
            if !still_auto {
                mark_stopped(&state, Some(pid));
                emit_status(&app, &qq, false, Some(pid), Some(code), cur_r, "已通过桌面端主动停止".to_string());
                break;
            }
            // 重新拉起：与首启走完全相同的入口，只差 announce_plugins=false。
            match spawn_napcat_process(&kind, &qq, &cwd, &app, false) {
                Ok(c) => {
                    let np = c.id();
                    *lock(&state.child) = Some(c);
                    mark_running(&state, np);
                    let new_count = {
                        let mut wg = lock(&state.watcher);
                        match wg.as_mut() {
                            Some(w) => { w.restart_count += 1; w.restart_count }
                            None => 0,
                        }
                    };
                    emit_status(&app, &qq, true, Some(np), Some(code), new_count,
                        format!("已自动拉起（第 {new_count} 次重启）"));
                    // 不 break：下一轮把新 child take 出来继续 wait()。
                }
                Err(e) => {
                    mark_stopped(&state, Some(pid));
                    emit_status(&app, &qq, false, Some(pid), Some(code), cur_r,
                        format!("自动重启失败: {e}"));
                    break;
                }
            }
        } else {
            // 正常退出 / 用户主动停止 / 超过最大重启次数 → 不再重启。
            let reason = if code == 0 {
                "进程正常退出".to_string()
            } else if !auto_restart {
                "已通过桌面端主动停止".to_string()
            } else {
                format!("连续崩溃 {cur_r} 次，已放弃自动重启")
            };
            mark_stopped(&state, Some(pid));
            emit_status(&app, &qq, false, Some(pid), Some(code), cur_r, reason);
            break;
        }
    });
}

// ===========================================================================
// Tauri commands
// ===========================================================================
