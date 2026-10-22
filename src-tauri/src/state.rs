// 全局运行状态（AppState / WatcherCtrl）
// 该模块的职责见 docs/ARCHITECTURE.md。
// 所有字段都是 pub(crate)：命令层与 watcher 线程都要直接读写它们。

use std::process::Child;
use std::sync::Mutex;

use crate::models::*;

/// watcher 线程与主线程共享的"控制开关 + 拉起参数"。
///
/// auto_restart 在用户点"停止"时被置 false，阻止 watcher 再次拉起；
/// 在"模拟崩溃"时被置 true，允许 watcher 自动重启以演示恢复流程。
/// kind/qq/cwd 是自动重启时照抄一遍的启动参数：watcher 只调用 spawn_napcat_process，
/// 不再自己拼命令行，从根上消除两套参数各写一份的漂移。
pub struct WatcherCtrl {
    pub auto_restart: bool,
    pub restart_count: u32,
    pub max_restarts: u32,
    /// 启动形态（bootmain / script），重启时原样复用
    pub kind: LaunchKind,
    pub qq: String,
    /// NapCat 包根目录（= cfg.napcat_path）。它不等于子进程的工作目录：
    /// bootmain 下真正的工作目录由 kind.cwd() 给出（shell\），
    /// quick-login.env 与 plugins\ 仍然从这个根目录往下找。
    pub cwd: String,
}

pub struct AppState {
    /// NapCat 子进程句柄槽，**只给 watcher 用**：watcher 会 take() 出去独占阻塞
    /// wait()（可能几小时），所以运行期间这里恒为 None。
    /// 判断"是否在运行"绝不能看这个槽，要看 running。
    pub child: Mutex<Option<Child>>,
    /// 运行状态真值，也是唯一的运行判据：
    ///   start_napcat / watcher 重启成功 → true；watcher 任何"不再重启"的出口 → false；
    ///   stop_napcat 后台线程确认进程消失 → false。
    /// 语义分工：running = 状态真值（给 UI 与守卫用）、current_pid = 操作句柄
    /// （给 taskkill / kill_napcat 用）、child = 只给 watcher wait() 的句柄槽。
    pub running: Mutex<bool>,
    /// 当前子进程 pid，单独存一份。
    /// 原因：watcher 会 take() 走 child，stop_napcat 时拿不到句柄，
    /// 但仍需要 pid 去 taskkill /T。
    pub current_pid: Mutex<Option<u32>>,
    pub watcher: Mutex<Option<WatcherCtrl>>,
    /// NoneBot bot.py 子进程 pid（附属机器人进程，不自动重启）
    pub bot_pid: Mutex<Option<u32>>,
    /// AI 自动回复网关（Node 子进程）的 pid。同样不自动重启：
    /// 它自己内置了 WS 断线重连，只有进程级崩溃才需要用户手动再启。
    pub gateway_pid: Mutex<Option<u32>>,
    /// 内存中的最新配置，save_config / load_config 都会同步它
    pub config: Mutex<AppConfig>,
}
