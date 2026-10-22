// 由 main.rs 拆分生成：NapCat 生命周期命令
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use crate::models::*;
use crate::services::*;
use crate::state::*;
use crate::utils::*;

/// 按镜像名列出进程 pid（解析 tasklist 的 CSV 输出）。
fn pids_by_name(image: &str) -> Vec<u32> {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image}"), "/FO", "CSV", "/NH"])
        .output();
    let Ok(o) = out else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            // 形如："QQ.exe","1234","Console","1","100,000 K"
            let parts: Vec<&str> = l.split(',').collect();
            if parts.len() < 2 {
                return None;
            }
            let name = parts[0].trim().trim_matches('"');
            if !name.eq_ignore_ascii_case(image) {
                return None;
            }
            parts[1].trim().trim_matches('"').parse::<u32>().ok()
        })
        .collect()
}

/// 清理上次没关干净的 NapCat 进程树，返回清理数量。
///
/// 为什么需要：面板状态是内存里的，进程是系统里的，两者会不同步。
///   · 用户手动用 launcher.bat 起过一个，面板并不知道；
///   · 上次强杀只带走了树的一部分，NapCatWinBootMain 成了孤儿。
/// 这些残留会一直占着 QQ 登录态，新实例起来就报「当前账号已登录，无法重复登录」。
///
/// 只杀 NapCatWinBootMain.exe 及其子树 —— 不碰用户的 QQ.exe：
/// 用户很可能同时开着自己的号在聊天，按镜像名全杀会把人踢下线。
fn cleanup_leftover_napcat(app: &tauri::AppHandle) -> usize {
    let pids = pids_by_name("NapCatWinBootMain.exe");
    if pids.is_empty() {
        return 0;
    }
    for p in &pids {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &p.to_string()])
            .status();
    }
    let msg = format!(
        "检测到 {} 个残留 NapCat 进程（PID: {:?}），已清理，等待 2 秒后启动",
        pids.len(),
        pids
    );
    emit_log(app, "warn", msg.clone());
    write_error_log(app, "warn", &msg);
    std::thread::sleep(Duration::from_secs(2));
    pids.len()
}

/// 启动 NapCat 子进程。
///
/// 入参：qq 可选，要登录的 QQ 号；不传则用配置里第一个 enabled 账号。
///
/// === 参数校验（安全审计 H1）===
///   - qq 必须是 5-11 位纯数字，否则直接 Err。QQ 号现实范围就是这个，
///     任何带字母/空格/& 的 qq 都不可能合法，直接拒掉，杜绝命令注入。
///   - launcher 路径在 launch_child 里会 canonicalize + 校验扩展名 + 校验目录树，
///     这里只负责用 resolve_launch 解析启动形态。
///
/// === 守卫为什么看 running 而不是 child ===
/// watcher 线程会把 child take() 出去独占 wait()（可能几小时），运行期间
/// state.child 恒为 None。若用 child 判"已在运行"，守卫几乎永不生效，用户重开面板
/// 看到"未运行"再点启动就会起第二个 NapCat 实例（QQ 重复登录冲突），
/// 还会覆盖 WatcherCtrl 造成两个 watcher 线程抢同一个 child 槽。
///
/// 副作用：spawn 子进程 + 挂日志转发线程（脱敏/分级/二维码）+ 启动 watcher 线程
/// + emit napcat://status。
///
/// 错误："QQ号必须是 5-11 位纯数字" / "NapCat 已在运行" / "NapCat 路径未配置..." /
/// "在 ... 下未找到启动脚本；..." / "spawn 失败: ..." / "启动 node.exe 失败: ..."。
#[tauri::command]
pub fn start_napcat(
    app: tauri::AppHandle, state: tauri::State<Arc<AppState>>, qq: Option<String>,
) -> Result<StatusInfo, String> {
    let st = state.inner().clone();
    // 守卫看 running 真值（见函数头注释），不看会被 watcher take 走的 child。
    if *lock(&st.running) {
        return Err("NapCat 已在运行".to_string());
    }
    let cfg = lock(&st.config).clone();
    if cfg.napcat_path.is_empty() {
        return Err("NapCat 路径未配置，请先在设置页选择或自动检测".to_string());
    }
    // 解析启动形态（bootmain 直启 node / launcher 脚本）。解析结果同时存进
    // WatcherCtrl，保证自动重启用完全相同的参数。
    let kind = resolve_launch(&cfg.napcat_path)?;

    let qq = qq.unwrap_or_else(|| {
        cfg.accounts.iter()
            .find(|a| a.enabled)
            .or_else(|| cfg.accounts.first())
            .map(|a| a.qq.clone())
            .unwrap_or_default()
    });

    // === qq 数字校验（H1 命令注入防护）===
    if !qq.is_empty() {
        let len = qq.len();
        if !(qq.chars().all(|c| c.is_ascii_digit()) && (5..=11).contains(&len)) {
            return Err("QQ号必须是 5-11 位纯数字".to_string());
        }
    }

    // 首启：announce_plugins=true，在日志里列一遍磁盘上扫到的插件
    // 启动前先清理残留：否则会撞上「当前账号已登录，无法重复登录」。
    cleanup_leftover_napcat(&app);
    let child = spawn_napcat_process(&kind, &qq, &cfg.napcat_path, &app, true)?;
    let pid = child.id();
    *lock(&st.child) = Some(child);
    mark_running(&st, pid);
    // watcher 自动重启时照抄这份参数：kind 决定形态，cwd 是包根目录
    // （子进程真正的工作目录由 kind.cwd() 推出，bootmain 是 shell\）。
    *lock(&st.watcher) = Some(WatcherCtrl {
        auto_restart: cfg.auto_restart,
        restart_count: 0,
        max_restarts: cfg.max_restarts,
        kind,
        qq: qq.clone(),
        cwd: cfg.napcat_path.clone(),
    });
    // 先 emit 一次"启动成功"，再让 watcher 接管。
    // 带 qq 字段，前端据此把对应账号卡片标记为"运行中"。
    emit_status(&app, &qq, true, Some(pid), None, 0, format!("已启动，QQ={qq}"));
    spawn_watcher(app.clone(), st.clone());

    Ok(StatusInfo {
        running: true,
        pid: Some(pid),
        last_exit_code: None,
        restart_count: 0,
        message: format!("已启动，QQ={qq}"),
    })
}

/// 主动停止 NapCat。会先把 watcher 的 auto_restart 关掉，
/// 再 taskkill /F /T 整棵进程树。这样 watcher 检测到子进程退出时不会自动拉起。
#[tauri::command]
pub fn stop_napcat(app: tauri::AppHandle, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let st = state.inner().clone();
    if let Some(w) = lock(&st.watcher).as_mut() { w.auto_restart = false; }
    // 从 current_pid 读（child 已被 watcher take 走，这里拿不到）
    let pid = *lock(&st.current_pid);
    if let Some(p) = pid {
        // 关闭确认流程放到后台线程：taskkill + sleep + tasklist 不能在主线程跑，
        // 否则会堵 500ms 导致 UI 未响应。
        let app2 = app.clone();
        std::thread::spawn(move || {
            // /T 连子进程一起杀，/F 强制结束。输出不直接读——
            // 中文系统 taskkill 输出是 GBK 编码，from_utf8_lossy 会乱码。
            let ok = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &p.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            // 等 500ms 让进程真正退出
            std::thread::sleep(Duration::from_millis(500));
            // 用 tasklist 确认进程是否还在
            let still = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {p}"), "/NH"])
                .output();
            let gone = match still {
                // tasklist 输出也是 GBK，但这里只判断 PID 数字是否出现，
                // 数字是 ASCII，不受编码影响。
                Ok(o2) => !String::from_utf8_lossy(&o2.stdout).contains(&p.to_string()),
                Err(_) => false,
            };
            if gone {
                emit_log(&app2, "ok", format!("已确认关闭 NapCat (PID={p})，进程树已退出"));
                // 服务端会话不会随进程立刻失效，QQ 侧通常要几十秒才判离线。
                emit_log(&app2, "info", "提示：若立即重启仍提示「已登录」，等待约 30 秒让 QQ 服务端释放会话");
                // 确认进程消失后才清运行状态；带 pid 守卫，避免误清新一轮启动的状态。
                mark_stopped(&st, Some(p));
            } else {
                let msg = format!("taskkill 已执行但进程仍存在 (PID={p})，可能被外部再次拉起");
                emit_log(&app2, "warn", msg.as_str());
                write_error_log(&app2, "warn", &msg);
            }
            // taskkill 退出码本身记一条（不读中文 stdout，避免乱码）
            if !ok {
                emit_log(&app2, "info", "taskkill 返回非零退出码，可能进程已自行退出");
            }
        });
        // 立即返回，不阻塞 UI
        emit_log(&app, "info", format!("正在关闭 NapCat (PID={p})..."));
    } else {
        emit_log(&app, "warn", "当前没有运行中的 NapCat 进程");
    }
    Ok(())
}

/// 模拟崩溃：保留 auto_restart=true，强制 kill 当前子进程。
/// watcher 会看到非零退出码，按指数退避自动拉起——用于演示"崩溃自动恢复"。
/// 未运行时返回错误 "NapCat 未在运行"。
#[tauri::command]
pub fn kill_napcat(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let st = state.inner().clone();
    // 确保 auto_restart 开着，否则 kill 完就不会拉起了。
    if let Some(w) = lock(&st.watcher).as_mut() { w.auto_restart = true; }
    // 先把 pid 取到局部变量再 match。若直接写成
    //   match *lock(&st.current_pid) { .. }
    // 作为函数尾表达式，MutexGuard 这个临时值会一直活到整条 match 结束才析构，
    // 而 st 在同一条语句末尾就已析构 → E0597（borrowed value does not live long enough）。
    // 拆成独立语句后，guard 在本行末尾即释放，借用不再跨越 st 的析构点。
    let pid = *lock(&st.current_pid);
    match pid {
        Some(p) => {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &p.to_string()])
                .status();
            Ok(())
        }
        None => Err("NapCat 未在运行".to_string()),
    }
}

/// 查询当前状态。前端轮询或启动时拉一次用。
/// running 读权威标志位、pid 读操作句柄，都不看 child 槽位：
/// watcher 独占 wait() 期间 child 恒为 None，看它会永远返回"未运行"。
#[tauri::command]
pub fn get_napcat_status(state: tauri::State<Arc<AppState>>) -> StatusInfo {
    let st = state.inner();
    let running = *lock(&st.running);
    let pid = *lock(&st.current_pid);
    let w = lock(&st.watcher);
    let restart_count = w.as_ref().map(|x| x.restart_count).unwrap_or(0);
    let auto = w.as_ref().map(|x| x.auto_restart).unwrap_or(false);
    StatusInfo {
        running,
        pid,
        last_exit_code: None,
        restart_count,
        message: if running {
            format!("运行中 PID={pid:?}（自动重启={auto}，已重启{restart_count}次）")
        } else {
            "未运行".to_string()
        },
    }
}

// ===========================================================================
// 窗口控制 command（前端自绘标题栏按钮通过 IPC 调用，不依赖 JS window API）
// 用 AppHandle 手动获取 main 窗口，避免参数注入失败。
// ===========================================================================
