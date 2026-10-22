// 模块聚合：对内声明子模块，对外统一重导出。
pub mod lock;
pub mod paths;
pub mod text;

pub use lock::*;
pub use paths::*;
pub use text::*;
