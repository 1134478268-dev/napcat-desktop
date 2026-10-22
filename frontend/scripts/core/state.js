/* 全局运行时状态与静态常量（state / 头像渐变 / 推荐插件清单）
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ============================================================================
 * NapCat Desktop · 前端全局状态
 * ----------------------------------------------------------------------------
 * 单一 state 对象 + 静态常量。视图只读它，写入收敛在 core/ipc.js 与 ui/ 层。
 * 初值只是空壳，真实数据由 load_config / napcat://status / get_bot_status 填充。
 *
 * 【为什么允许全局单例】前端零构建、无状态库。拆成多实例会让"渲染时读哪份
 *   状态"变成隐式约定，比一个显式单例更难追。如果要引入状态库，这里是唯一入口。
 *
 * 【安全约定】凡来自后端/用户的字符串（QQ 备注、插件名、日志行、网络地址），
 *   拼进 innerHTML 前必须过 escapeHtml()——否则用户在备注里写
 *   `<img src=x onerror=alert(1)>` 就会在桌面端执行脚本。
 * ========================================================================== */

/* ================= State（运行时状态） =================
 * 由 load_config() + napcat://status 事件填充，初值仅为空壳。 */
const state = {
  running: false,
  /* 当前正在运行的 QQ 号（napcat://status 的 qq 字段；未运行/后端未给时为空串）。
   * 为什么不复用 accounts[i].enabled：enabled 是"启动时是否拉起这个号"的持久配置，
   *   "正在运行"是子进程的瞬时事实。两者混用会在保存时把运行态写回配置文件，
   *   把除当前号以外的账号全部写成 enabled:false 永久禁用（见 applyStatus 的历史注释）。 */
  runningQq: '',
  crashed: false,
  startedAt: null,
  accounts: [],
  services: [],
  plugins: [],
  logs: [],
  /* 暂停 = 停止"自动增量重绘 + 自动滚动"，但日志照收（见 togglePauseLogs） */
  pausedLogs: false,
  /* 暂停期间新到日志的条数，只用于在暂停按钮上提示"积压了多少"，恢复时清零 */
  pausedCount: 0,
  /* OneBot 表单状态：null=无表单；{i,type,port,addr,token,memo,editing} 表示正在新建/编辑。
   * i=null 表示新建；i=数字 表示编辑第 i 条。 */
  netForm: null,
  /* NoneBot 机器人进程是否在运行（后端 get_bot_status 返回） */
  botRunning: false,
  /* 当前磁盘配置的内存镜像（load_config 返回值原样保存）；
   * 所有"保存"都是改它的字段后整体 save_config 覆盖写回（后端不支持增量 patch） */
  cfg: null,
  /* AI 接入页状态。config/personas/memories 由 ai_load_state 一次取回，
   * 改动后整体 save_* 覆盖写盘（与 cfg 同一套思路，避免增量补丁的中间态）。 */
  ai: {
    loaded: false,
    config: null,
    personas: [],
    memories: [],
    chat: [],           // {role:'user'|'assistant', content, error?}
    busy: false,        // 请求进行中，禁用发送按钮
    providerForm: null, // 正在编辑的 provider 草稿；{id?, ...} 或 null
    personaForm: null,  // 正在编辑的人设草稿
    gatewayStatus: {},  // 自动回复网关运行状态 {running,pid,message}
    loadFailed: false   // ai_load_state 失败时置 true，禁止任何写回
  }
};

/* 账号头像渐变色板：QQ 号稳定 hash 选色，避免每次 render 颜色跳变 */
const AVATAR_GRADIENTS = [
  'linear-gradient(135deg,#5ab0ff,#a06bff)',
  'linear-gradient(135deg,#ff9f43,#ff6b6b)',
  'linear-gradient(135deg,#3ddc97,#5ab0ff)',
];

/* ================= 推荐插件清单（可选插件） =================
 * 来源：NapCat 官方插件索引（NapNeko/napcat-plugin-index 的 plugins.v4.json，
 * 抓取于 2026-10）＋ GitHub 上按 star 排序的社区 napcat-plugin 仓库。
 *
 * 为什么把清单硬编码在这里，而不是运行时去 GitHub 拉：
 *   1) 面板的 CSP 只允许 'self' 与 ipc，前端 fetch 不到 github.com；
 *   2) 就算让后端代拉，本机常见网络环境下 github.com 直连会超时（实测 21s 超时），
 *      插件页会因为拿不到清单而整页空白——这对"离线也要能用"的桌面面板不可接受；
 *   3) 硬编码可以人工精选并写清用途，也不会因上游改名/删库而白屏。
 *
 * 字段：
 *   id       安装后的目录名（传给 install_plugin_from_url 的 name）
 *   zip      可直接下载安装的包地址；**没有 zip 的表示该仓库没提供构建好的产物**，
 *            面板只提供「仓库」按钮，由用户按仓库说明自行构建安装（不假装能一键装）
 *   official 是否 NapNeko 官方出品，用来做视觉区分
 */
const RECOMMENDED_PLUGINS = [
  { id:'napcat-plugin-builtin', name:'内置插件', author:'NapNeko', official:true,
    desc:'NapCat 官方内置插件，提供基础能力扩展。',
    repo:'https://github.com/NapNeko/napcat-plugin-index',
    zip:'https://github.com/NapNeko/napcat-plugin-index/releases/download/v1.0.0/napcat-plugin-builtin.zip' },
  { id:'napcat-plugin-cleaner', name:'缓存清理', author:'NapNeko', official:true,
    desc:'定期清理 NapCat 运行产生的缓存文件，避免目录无限膨胀。',
    repo:'https://github.com/NapNeko/napcat-plugin-index',
    zip:'https://github.com/NapNeko/napcat-plugin-index/releases/download/v1.0.0/napcat-plugin-cleaner.zip' },
  { id:'napcat-plugin-qce', name:'QQ 聊天记录导出', author:'shuakami', official:false,
    desc:'把聊天记录导出为 HTML / JSON / TXT / Excel，支持时间筛选、批量与定时任务、表情图片语音资源处理。',
    repo:'https://github.com/shuakami/qq-chat-exporter',
    zip:'https://github.com/shuakami/qq-chat-exporter/releases/download/v5.5.77/napcat-plugin-qce.zip' },
  { id:'napcat-plugin-ssqq', name:'Stapxs QQ Lite', author:'林小槐', official:false,
    desc:'兼容 OneBot 协议的第三方 QQ 客户端，可以在浏览器里独立收发消息。',
    repo:'https://github.com/Stapxs/Stapxs-QQ-Lite-2.0',
    zip:'https://github.com/Stapxs/Stapxs-QQ-Lite-2.0/releases/download/v3.5.2/napcat-plugin-ssqq.zip' },
  { id:'napcat-plugin-debug', name:'插件调试服务', author:'NapNeko', official:true,
    desc:'通过 WebSocket 暴露 PluginManager API，配合 CLI 实现插件热重载，开发插件时用。',
    repo:'https://github.com/NapNeko/napcat-plugin-debug',
    zip:'https://github.com/NapNeko/napcat-plugin-debug/releases/download/v1.0.8/napcat-plugin-debug.zip' },
  { id:'napcat-plugin-bilibili', name:'B 站解析', author:'AQiaoYo', official:false,
    desc:'自动解析消息里的 bilibili 链接（视频 / 番剧 / 直播），输出标题与封面。',
    repo:'https://github.com/AQiaoYo/napcat-plugin-bilibili' },
  { id:'napcat-plugin-group-manager', name:'群管助手', author:'ChaceQC', official:false,
    desc:'群成员管理相关命令集合（禁言、踢人、头衔等）。',
    repo:'https://github.com/ChaceQC/napcat-plugin-group-manager' },
  { id:'napcat-plugin-checkin', name:'群签到', author:'MY-Final', official:false,
    desc:'群内签到 / 积分玩法插件。',
    repo:'https://github.com/MY-Final/napcat-plugin-checkin' },
  { id:'napcat-plugin-gscore-adapter', name:'GScore 适配器', author:'xiowo', official:false,
    desc:'把 NapCat 接入 GScore 框架的适配器。',
    repo:'https://github.com/xiowo/napcat-plugin-gscore-adapter' },
  { id:'openclaw-napcat-plugin', name:'OpenClaw AI 通道', author:'ProperSAMA', official:false,
    desc:'把 NapCat 当作 OpenClaw AI 助手的消息通道（社区 star 数最高的 napcat-plugin 之一）。',
    repo:'https://github.com/ProperSAMA/openclaw-napcat-plugin' },
];

/* 演示日志语料与随机造流已移除：真实日志由 napcat://log 事件逐行推送（见文件末尾 boot()）。 */
