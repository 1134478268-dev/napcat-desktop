// NapCat Desktop - Tauri 主进程
// ---------------------------------------------------------------------------
// 模块职责（整个后端就这一个文件）：
//   1. 窗口与系统托盘管理（保持原逻辑：关闭最小化到托盘）
//   2. 配置持久化（app_config_dir/napcat-desktop.json）
//   3. NapCat / QQNT 安装路径检测（含注册表读取）
//   4. 插件目录扫描 / 安装 / 备注，NoneBot bot.py 子进程管理
//   5. NapCat 子进程拉起 / 停止 / 状态查询 / 崩溃自动拉起（指数退避）
//   6. 子进程 stdout/stderr 转发为 Tauri 事件给前端日志视图（含 token 脱敏）
//
// 关键类型：
//   - AppConfig  ：磁盘上的 JSON 配置结构，字段名与前端一一对齐。少一个字段不会报错，
//                  只会静默丢数据（OneBot 的 token 就这么丢过一次）。
//   - AppState   ：全局可变状态，挂 tauri manage 上用 Arc 包一层给 watcher 线程克隆。
//                  running = 运行状态真值，current_pid = 操作句柄，child = watcher 句柄槽。
//   - WatcherCtrl：watcher 与主线程共享的"自动重启开关 + 重启计数 + 启动形态 + 拉起参数"。
//   - LaunchKind ：NapCat 的两种启动形态（bootmain 直启 node / launcher 脚本）。
//                  首启与自动重启共用同一份解析结果，避免两条路径参数漂移。
//
// 对外暴露的 Tauri command（共 20 个，名字与签名是对前端的契约，不可改）：
//   ping / load_config / save_config
//   detect_napcat_path / detect_qq_path / pick_directory
//   list_plugins / install_plugin
//   start_bot / stop_bot / get_bot_status
//   start_napcat / stop_napcat / kill_napcat / get_napcat_status
//   win_minimize / win_toggle_max / win_close
//   open_external / export_logs
//
// 对外发送的 Tauri 事件（事件名与 payload 键同样是对前端的契约）：
//   napcat://log    { line, level }   level ∈ info / ok / warn / err / recv / plugin
//   napcat://status { running, pid, qq, last_exit_code, restart_count, message }
//                   前端靠 qq 判断"当前哪个账号在跑"，一个键都不能少
//   napcat://qrcode { data_url }      扫码登录用的二维码 PNG data URL
// ---------------------------------------------------------------------------

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod models;
mod services;
mod state;
mod utils;

use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    Manager, WindowEvent,
};

use commands::*;
use models::*;
use services::*;
use state::*;

fn main() {
    tauri::Builder::default()
        .manage(Arc::new(AppState {
            child: Mutex::new(None),
            running: Mutex::new(false),
            current_pid: Mutex::new(None),
            bot_pid: Mutex::new(None),
            gateway_pid: Mutex::new(None),
            watcher: Mutex::new(None),
            config: Mutex::new(AppConfig::default()),
        }))
        .invoke_handler(tauri::generate_handler![
            ping,
            load_config,
            save_config,
            detect_napcat_path,
            detect_qq_path,
            pick_directory,
            list_plugins,
            install_plugin,
            install_plugin_from_url,
            start_bot,
            stop_bot,
            kill_bot,
            get_bot_status,
            start_napcat,
            stop_napcat,
            kill_napcat,
            get_napcat_status,
            win_minimize,
            win_toggle_max,
            win_close,
            open_external,
            export_logs,
            // ---- AI 接入 ----
            ai_load_state,
            ai_save_config,
            ai_save_personas,
            ai_save_memories,
            ai_chat,
            ai_gateway_start,
            ai_gateway_stop,
            ai_gateway_status,
        ])
        .setup(|app| {
            // 系统托盘（保持原有逻辑）
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            // 必须把 TrayIcon 句柄交给 app 托管：Tauri 2 的 TrayIcon 实现了 Drop，
            // 绑定成局部变量会在 setup 闭包返回时被 drop —— 托盘图标随之被移除。
            // 用 app.manage 挂到全局状态，图标才会一直存在。
            // 图标缺失属于打包配置错误，这里用 ? 报出明确原因，而不是 unwrap panic。
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("缺少默认窗口图标：检查 tauri.conf.json 的 bundle.icon")?;
            let tray: TrayIcon = TrayIconBuilder::with_id("main")
                .tooltip("NapCat Desktop")
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        // 退出前必须把机器人子进程一起带走，否则 python bot.py 会变成
                        // 孤儿继续占着 8080 端口，下次启动直接 bind 失败。
                        // 这里拿不到 tauri::State，用 app.state() 取全局托管的状态。
                        let st = app.state::<Arc<AppState>>().inner().clone();
                        let killed = force_kill_bot(&st);
                        if !killed.is_empty() {
                            emit_log_from(app, SRC_BOT, "warn", format!("[bot] 退出前已清理机器人进程: {:?}", killed));
                        }
                        // 网关同理：它是独立进程，不带走的话下次启动会再起一个，
                        // 变成两个网关联同一条 WS，同一条消息被回复两次。
                        if let Err(e) = services::ai_gateway::stop(&st) {
                            let _ = e; // 本来就没在运行，属于正常情况
                        }
                        app.exit(0)
                    }
                    _ => {}
                })
                .build(app)?;
            // 交给 app 托管，避免句柄在 setup 结束时 drop 导致托盘图标消失
            app.manage(tray);

            // 开机自动拉起 AI 网关：只有配置里 auto_start && enabled 时才启动。
            // 失败不影响面板（常见原因是没装 Node），只记一条 warn，用户可在 AI 页手动重试。
            {
                let st = app.state::<Arc<AppState>>().inner().clone();
                if let Ok(ai) = services::ai_service::load_state(app.handle()) {
                    let g = &ai.config.gateway;
                    if g.auto_start && g.enabled {
                        if let Err(e) = services::ai_gateway::start(app.handle().clone(), st) {
                            emit_log_from(app.handle(), SRC_AI, "warn", format!("[ai] 自动启动网关失败: {e}"));
                        }
                    }
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 点 X 最小化到托盘而不是退出
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running NapCat Desktop");
}

// ===========================================================================
// 单元测试：日志脱敏（cargo test）
// ===========================================================================
//
// 为什么专门给 redact 写测试：它曾经有一个"替换后 key 仍在、while 永不前进"的
// 死循环，而它跑在**读取子进程 stdout 的线程**里 —— 一旦死循环，该线程不再消费
// 管道，缓冲区填满会反过来卡死 NapCat 自身，而且**不会 panic**、静默停摆极难排查。
// 下面把三类回归固定下来：①必须终止 ②不能泄漏凭据 ③多字节日志不能 panic
// （Cargo.toml 里 panic="abort"，日志路径上一次 panic 就是整个应用 abort）。
#[cfg(test)]
mod redact_tests {
    use crate::utils::text::redact;

    #[test]
    fn redacts_common_forms() {
        assert_eq!(redact("token=abc123"), "token=***");
        assert_eq!(redact("access_token=abc123"), "access_token=***");
        assert_eq!(redact("Authorization: Bearer sk-live-xyz"), "Authorization: Bearer ***");
        assert_eq!(redact("TOKEN=sk-upper"), "TOKEN=***"); // 大小写不敏感
        assert_eq!(redact("authorization: sk-only"), "authorization: ***"); // 小写 + 冒号
        assert_eq!(redact("x-api-key: sk-key"), "x-api-key: ***");
        assert_eq!(redact("qq_password=hunter2"), "qq_password=***");
    }

    #[test]
    fn redacts_quoted_and_json_values_without_eating_neighbours() {
        // 引号保留、只把值打成 ***，且不能把后面的字段一起吞掉
        assert_eq!(redact("token=\"quoted\""), "token=\"***\"");
        assert_eq!(
            redact("{\"token\":\"sk-json\",\"msg\":\"hi\"}"),
            "{\"token\":\"***\",\"msg\":\"hi\"}"
        );
    }

    #[test]
    fn redacts_every_occurrence() {
        assert_eq!(redact("token=a&access_token=b"), "token=***&access_token=***");
        assert_eq!(redact("token=a token=b"), "token=*** token=***");
    }

    #[test]
    fn terminates_when_key_has_no_value() {
        // 这三条正是修复前会让 while 原地打转（死循环）的输入
        assert_eq!(redact("token="), "token=");
        assert_eq!(redact("token=   "), "token=   ");
        assert_eq!(redact("Authorization:"), "Authorization:");
    }

    #[test]
    fn does_not_touch_plain_words_containing_key() {
        // 键名后面必须紧跟分隔符，否则只是普通单词的一部分，不能误脱敏正文
        assert_eq!(redact("tokenizer is loading"), "tokenizer is loading");
        assert_eq!(redact("passwordless login"), "passwordless login");
    }

    #[test]
    fn survives_multibyte_text_without_panic() {
        let out = redact("Authorization: 中文令牌内容");
        assert!(out.starts_with("Authorization: ***"), "got: {out}");
        let out2 = redact("备注 token=秘密 结束");
        assert!(out2.contains("token=***"), "got: {out2}");
    }

    #[test]
    fn leaves_plain_lines_untouched() {
        assert_eq!(redact("普通日志，没有凭据"), "普通日志，没有凭据");
    }
}

/// 临时诊断：验证真实 ai/config.json 能否被反序列化。
#[cfg(test)]
mod ai_config_tests {
    #[test]
    fn parses_real_ai_config_if_present() {
        let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("com.napcat.desktop")
            .join("ai")
            .join("config.json");
        if !path.exists() {
            eprintln!("skip: {} not found", path.display());
            return;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let cfg: crate::models::AiConfig =
            serde_json::from_str(&text).expect("ai config 反序列化失败");
        assert!(!cfg.providers.is_empty(), "providers 解析成了空");
        eprintln!("ok providers={} gateway.enabled={}", cfg.providers.len(), cfg.gateway.enabled);
    }
}
