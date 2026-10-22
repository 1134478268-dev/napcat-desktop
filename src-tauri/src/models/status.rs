// 由 main.rs 拆分生成：运行状态结构（StatusInfo）
// 该模块的职责见 docs/ARCHITECTURE.md。

use serde::Serialize;


/// get_napcat_status 返回的运行时状态。
#[derive(Clone, Serialize, Debug)]
pub struct StatusInfo {
    pub running: bool,
    pub pid: Option<u32>,
    /// 最近一次退出码；从未退出过为 null
    pub last_exit_code: Option<i32>,
    /// 本次启动会话内已经自动重启过几次
    pub restart_count: u32,
    /// 给前端展示的一句话状态
    pub message: String,
}

// ===========================================================================
// 全局可变状态
// ===========================================================================
