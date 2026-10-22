// 模块聚合：对内声明子模块，对外统一重导出。
pub mod ai_gateway;
pub mod ai_service;
pub mod detect;
pub mod events;
pub mod launcher;
pub mod logging;
pub mod plugin_fs;
pub mod watcher;

// 注意：ai_service 不做 glob 重导出。它的 `save_*` 与 commands::app::save_config
// 同名，glob 出去会在 invoke_handler 里产生歧义（E0659），命令层统一用
// `ai_service::xxx` 限定路径调用。
pub use detect::*;
pub use events::*;
pub use launcher::*;
pub use logging::*;
pub use plugin_fs::*;
pub use watcher::*;
