// 模块聚合：对内声明子模块，对外统一重导出。
pub mod ai;
pub mod app;
pub mod bot;
pub mod detect;
pub mod napcat;
pub mod plugin;
pub mod system;
pub mod window;

pub use ai::*;
pub use app::*;
pub use bot::*;
pub use detect::*;
pub use napcat::*;
pub use plugin::*;
pub use system::*;
pub use window::*;
