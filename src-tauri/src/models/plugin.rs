// 由 main.rs 拆分生成：插件展示结构（PluginInfo）
// 该模块的职责见 docs/ARCHITECTURE.md。

use serde::Serialize;


/// list_plugins 返回的单个插件信息。
#[derive(Clone, Serialize, Debug)]
pub struct PluginInfo {
    /// 插件目录名（去除 .disabled 后缀）
    pub dir: String,
    /// 从 package.json 读出的 displayName / name，读不到就退回目录名
    pub name: String,
    pub version: String,
    pub desc: String,
    pub author: String,
    /// 是否启用（目录名没有 .disabled 后缀即视为启用）
    pub enabled: bool,
    /// 是否检测到可执行入口（index.js / main.js / main.py）
    pub has_exec: bool,
}
