// 由 main.rs 拆分生成：配置数据结构（AppConfig / Account / OneBotService）
// 该模块的职责见 docs/ARCHITECTURE.md。

use serde::{Deserialize, Serialize};


/// 一个 QQ 账号条目。saved_login 表示"桌面端是否已记住扫码登录态"。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Account {
    pub qq: String,
    pub nick: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub saved_login: bool,
}

/// OneBot 网络服务条目（HTTP / WS / POST-WS）。
///
/// token 必须显式声明：前端 save_config 会带 token 字段、load_config 之后也会读它，
/// 而 serde 默认**忽略未知字段**、也**不输出未声明的字段**。少了这一行不会报错，
/// 只会让用户填的 OneBot 鉴权 token 每次保存都被静默丢掉，前端看到的是"填了又没了"。
/// 字段名/类型必须和前端严格一致。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OneBotService {
    #[serde(rename = "type")]
    pub tp: String,
    pub port: u16,
    pub addr: String,
    /// OneBot 鉴权 token（服务端口的访问令牌）。旧配置没有这个键，用 default 向前兼容。
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub memo: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

pub fn default_true() -> bool { true }
pub fn default_max_restarts() -> u32 { 5 }

/// 整个桌面端的配置，序列化为 napcat-desktop.json。
/// 所有字段都带 serde(default)，保证旧版本配置文件能向前兼容。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppConfig {
    /// NapCat 工作目录（launcher.bat 所在目录）
    #[serde(default)]
    pub napcat_path: String,
    /// QQNT 安装目录（仅用于兼容性提示，不强制使用）
    #[serde(default)]
    pub qq_path: String,
    /// 插件目录，一般是 <napcat_path>/plugins
    #[serde(default)]
    pub plugins_dir: String,
    /// 崩溃后是否自动拉起
    #[serde(default = "default_true")]
    pub auto_restart: bool,
    /// 单次启动会话内最多连续自动重启次数，超过后放弃
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    #[serde(default)]
    pub start_on_boot: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub onebot: Vec<OneBotService>,
    /// 插件备注：key = 插件目录名，value = 用户填的备注
    #[serde(default)]
    pub plugin_memos: std::collections::BTreeMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            napcat_path: String::new(),
            qq_path: String::new(),
            plugins_dir: String::new(),
            auto_restart: true,
            max_restarts: 5,
            start_on_boot: false,
            minimize_to_tray: true,
            proxy: String::new(),
            accounts: Vec::new(),
            onebot: Vec::new(),
            plugin_memos: std::collections::BTreeMap::new(),
        }
    }
}
