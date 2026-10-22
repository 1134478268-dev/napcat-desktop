// 模块聚合：对内声明子模块，对外统一重导出。
pub mod ai;
pub mod config;
pub mod launch;
pub mod plugin;
pub mod status;

pub use ai::*;
pub use config::*;
pub use launch::*;
pub use plugin::*;
pub use status::*;
