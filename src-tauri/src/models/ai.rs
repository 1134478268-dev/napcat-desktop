//! AI 接入的数据结构。
//!
//! 设计取舍：
//! - 配置分三层：provider（接哪家）、persona（人设）、memory（记忆）。
//!   三者独立存文件，用户改人设不会碰 provider 的密钥。
//! - provider 用 `kind` 区分协议族（openai / anthropic / gemini），
//!   而不是按厂商枚举：国内大部分模型都兼容 OpenAI 协议，新增厂商只需加一条配置。
//! - `tool_calls` 现在是空数组，是给 function calling 预留的出口；
//!   前端按数组处理，后端将来填空，接口不用改。

use serde::{Deserialize, Serialize};

/// 一家 AI 服务的接入配置。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AiProvider {
    /// 稳定标识（前端用它引用），建议用厂商名
    pub id: String,
    /// 展示名
    pub name: String,
    /// 协议族：openai / anthropic / gemini
    #[serde(default = "default_kind")]
    pub kind: String,
    /// API 根地址，例如 https://api.deepseek.com/v1
    pub api_base: String,
    /// 密钥。只存在本地配置文件里，前端展示时打码。
    #[serde(default)]
    pub api_key: String,
    /// 模型名，例如 deepseek-chat / gpt-4o / claude-sonnet-4-5
    pub model: String,
    #[serde(default = "crate::models::config::default_true")]
    pub enabled: bool,
}

fn default_kind() -> String {
    "openai".to_string()
}

/// 一条人设。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Persona {
    pub id: String,
    pub name: String,
    /// 系统提示词正文
    pub prompt: String,
    #[serde(default)]
    pub created_at: String,
}

/// 一条长期记忆。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MemoryEntry {
    pub id: String,
    /// 归属人设；空串表示对所有人生效
    #[serde(default)]
    pub persona_id: String,
    pub content: String,
    #[serde(default)]
    pub created_at: String,
    /// manual = 用户手写；auto = 将来由模型归纳
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "manual".to_string()
}

/// AI 全局配置。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AiConfig {
    /// 总开关：关闭后 AI 页进入禁用态，网关强制停止，所有 AI 调用拒绝执行。
    #[serde(default = "crate::models::config::default_true")]
    pub enabled: bool,
    /// 当前使用的 provider id
    #[serde(default)]
    pub active_provider: String,
    #[serde(default)]
    pub providers: Vec<AiProvider>,
    /// 当前使用的人设 id
    #[serde(default)]
    pub active_persona: String,
    /// 所有人设之外的系统级补充说明（可留空）
    #[serde(default)]
    pub system_extra: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// 是否把记忆拼进 system prompt
    #[serde(default = "crate::models::config::default_true")]
    pub memory_enabled: bool,
    /// 记忆条数上限，超出按时间保留最新的
    #[serde(default = "default_memory_limit")]
    pub memory_limit: u32,
    /// 工具调用开关。当前后端未实现工具执行，打开只会把工具清单放进提示词。
    #[serde(default)]
    pub tool_calling_enabled: bool,
    /// 自动回复网关配置
    #[serde(default)]
    pub gateway: AiGateway,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    2048
}

fn default_memory_limit() -> u32 {
    50
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            active_provider: String::new(),
            providers: Vec::new(),
            active_persona: String::new(),
            system_extra: String::new(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            memory_enabled: true,
            memory_limit: default_memory_limit(),
            tool_calling_enabled: false,
            gateway: AiGateway::default(),
        }
    }
}

/// AI 自动回复网关的配置。
///
/// 网关本身是一个独立的 Node 进程（见 `assets/ai-gateway.mjs`）：
/// 它连 NapCat 的正向 WS 收消息、调 AI、把回复通过 HTTP 发回去。
/// 放在独立进程里有三个好处：
///   1. 不占用 Tauri 主线程，也不把 WS 客户端塞进 Rust 依赖树；
///   2. 崩了只影响网关，NapCat 面板还能正常用；
///   3. 日志天然走已有的 stdout 转发链路（脱敏、分级、落盘全部复用）。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AiGateway {
    /// 总开关：关掉后不会回复任何消息
    pub enabled: bool,
    /// 桌面端启动时自动拉起网关
    pub auto_start: bool,
    /// NapCat 正向 WS 地址
    pub ws_url: String,
    /// NapCat HTTP API 地址（用于发消息、下载图片）
    pub http_url: String,
    /// OneBot access token，需与 NapCat 的 onebot11 配置一致
    pub access_token: String,
    /// 是否回复私聊
    pub reply_private: bool,
    /// 群里是否只在被 @ 时回复
    pub reply_group_at: bool,
    /// 上下文保留的对话轮数（一问一答算一轮）
    pub context_rounds: u32,
    /// 同一会话两次回复之间的最小间隔，防刷屏
    pub cooldown_ms: u32,
    /// 回复最大字符数，超出截断
    pub max_reply_chars: u32,
    /// 是否保存收到的图片到本地表情库
    pub save_stickers: bool,
    /// 允许 AI 在回复里用 [sticker:名字] 发送本地表情
    pub send_stickers: bool,
}

impl Default for AiGateway {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_start: false,
            ws_url: "ws://127.0.0.1:3001".to_string(),
            http_url: "http://127.0.0.1:3000".to_string(),
            access_token: String::new(),
            reply_private: true,
            reply_group_at: true,
            context_rounds: 10,
            cooldown_ms: 1500,
            max_reply_chars: 800,
            save_stickers: true,
            send_stickers: true,
        }
    }
}

/// 网关运行状态。
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct AiGatewayStatus {
    pub running: bool,
    pub pid: Option<u32>,
    /// 最近一次失败原因（启动失败 / 退出码非零）
    #[serde(default)]
    pub message: String,
}

/// 一次对话消息。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    /// user / assistant / system
    pub role: String,
    pub content: String,
}

/// 对话结果。
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct AiChatResult {
    pub content: String,
    pub model: String,
    /// 上游返回的 token 用量，协议不同可能缺字段
    #[serde(default)]
    pub prompt_tokens: Option<u32>,
    #[serde(default)]
    pub completion_tokens: Option<u32>,
    /// 预留：模型要求执行的工具调用。当前恒为空。
    #[serde(default)]
    pub tool_calls: Vec<serde_json::Value>,
}

/// 前端一次加载的完整 AI 状态。
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct AiState {
    pub config: AiConfig,
    pub personas: Vec<Persona>,
    pub memories: Vec<MemoryEntry>,
}
