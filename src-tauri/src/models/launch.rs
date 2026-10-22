// 由 main.rs 拆分生成：启动形态枚举（LaunchKind）
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::path::PathBuf;


/// NapCat 的启动形态。
///
/// 为什么要有它：NapCat 的打包方式不统一，桌面端要同时支持
///   - bootmain 整合包：<root>\shell\ 自带 node.exe + index.js，直接起 node 才绕得开
///     bat 中文编码乱码、ps1 拉起后立即退出的问题；
///   - 官方 / 通用包：根目录的 launcher 脚本，必须经 cmd / pwsh / CreateProcess。
///
/// 首启与崩溃自动重启必须复用同一个解析结果，否则会出现"首启 cwd 是 shell\、
/// 重启 cwd 变成包根目录"这类不一致（历史上真的存在）。
#[derive(Clone, Debug)]
pub enum LaunchKind {
    /// 直接 spawn node.exe index.js [-q <qq>]，工作目录 = shell\
    Bootmain { node_exe: PathBuf, index_js: PathBuf, shell_dir: PathBuf },
    /// 走 launcher 脚本（.bat 经 cmd、.ps1 经 pwsh、.exe 直接起）
    Script { launcher: PathBuf },
}

impl LaunchKind {
    /// 子进程的工作目录：bootmain 必须是 shell\（node 按相对路径找 ./config ./plugins），
    /// launcher 脚本则在包根目录执行。
    pub fn cwd(&self, napcat_path: &str) -> String {
        match self {
            LaunchKind::Bootmain { shell_dir, .. } => shell_dir.to_string_lossy().to_string(),
            LaunchKind::Script { .. } => napcat_path.to_string(),
        }
    }
}
