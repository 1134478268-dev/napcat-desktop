/* 按钮动作表与通用操作（打开外部链接 / 插件安装 / 导出日志 / 状态徽章）
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ============================================================================
 * 统一按钮动作系统：所有按钮用 data-action="xxx" 标注，不写 inline onclick。
 * document 上做一次性 click 委托，根据 data-action 分发到下面的 actions map。
 * 参数通过 data-qq / data-i / data-key 等 data-* 属性传递，由 handler 读取。
 * ========================================================================== */

const actions = {
  /* ---- 标题栏 ---- */
  'win-minimize': () => callCmd('win_minimize'),
  'win-max': () => callCmd('win_toggle_max'),
  'win-close': () => callCmd('win_close'),

  /* ---- 顶栏 ---- */
  'toggle-theme': () => toggleTheme(),
  'check-update': () => toast('NapCat Desktop v0.2.1 · 当前已是最新'),

  /* ---- 概览 ---- */
  'start': () => callCmd('start_napcat'),
  'stop': () => callCmd('stop_napcat'),
  'restart': () => restartNapCat(),
  /* 模拟崩溃（仅调试入口）：走后端专门留的 kill_napcat，保留 auto_restart 语义——
   * watcher 看到非零退出码会按指数退避自动拉起，正好演示崩溃恢复。
   * 未运行时后端 reject "NapCat 未在运行"，callCmd 已 toast。
   * 不回填 state：napcat://status(running=false, exit_code≠0) 会回流成崩溃 UI。 */
  'simulate-crash': () => callCmd('kill_napcat'),
  /* 一键拉起（崩溃后的恢复按钮）：start_napcat 复用本地登录态，结果同样由 status 事件驱动 */
  'wake': () => callCmd('start_napcat'),
  'go-logs': () => go('logs'),

  /* ---- 账号 ---- */
  'acct-add': () => addAccount(),
  'acct-delete': (el) => deleteAccount(el.dataset.qq),
  'acct-toggle': (el) => toggleAcct(el.dataset.qq),
  'acct-enable': (el) => toggleAcctEnabled(el.dataset.qq),
  'acct-edit-nick': (el) => editNick(Number(el.dataset.i)),

  /* ---- OneBot 网络 ---- */
  'net-edit': (el) => openEditNet(Number(el.dataset.i)),
  'net-new': () => openNewNet(),
  'net-delete': (el) => deleteNet(Number(el.dataset.i)),
  'net-toggle': (el) => toggleNet(Number(el.dataset.i)),
  'net-save': () => saveNet(),
  'net-cancel': () => cancelNet(),

  /* ---- 插件 ---- */
  /* 没有插件启停 action：本面板不提供插件启停（后端无此命令），卡片上是只读状态标签。
   * 启停请走 NapCat 自带 WebUI 插件市场。 */
  'bot-toggle': async () => {
    try {
      if(state.botRunning){
        await callCmd('stop_bot');
        toast('正在停止机器人…');
        state.botRunning = false;
        render();
      } else {
        // start_bot 成功才会 resolve；找不到 bot.py / spawn 失败会 reject
        await callCmd('start_bot');
        toast('机器人启动成功', 'ok');
        state.botRunning = true;
        render();
      }
    } catch(e){
      const msg = (e && e.message) ? e.message : String(e);
      toast('机器人启动失败: ' + msg, 'warn');
    }
  },
  /* 强制结束：不判断"是否在运行"，直接把机器人进程连同残留的 bot.py 一起杀掉。
   * 场景：直接关掉程序 / 程序被强杀后，python bot.py 会变成孤儿占着 8080，
   * 导致下次启动报 [Errno 10048] 而立刻退出。这个按钮就是清这种残留的。 */
  'bot-kill': async () => {
    try {
      const msg = await callCmd('kill_bot');
      toast(msg || '已强制结束机器人进程', 'ok');
      state.botRunning = false;
      render();
    } catch(e){
      const msg = (e && e.message) ? e.message : String(e);
      toast('强制结束失败: ' + msg, 'warn');
    }
  },
  'plugin-edit-memo': async (el) => {
    const i = Number(el.dataset.i);
    const p = state.plugins[i];
    if(!p) return;
    const cur = (state.cfg.plugin_memos||{})[p.dir] || '';
    const v = await showModal(`编辑插件备注`, p.name, cur);
    if(v !== null){
      await savePluginMemo(p.dir, v);
      render();
    }
  },
  'plugin-refresh': () => refreshPlugins(),
  'plugin-open-dir': () => openPluginsDir(),
  /* 推荐插件（可选插件区）：安装 / 打开仓库。
   * 参数走 data-*（id/name/url），不拼进 HTML 属性之外的任何地方，
   * 且最终由后端做域名白名单与目录名白名单二次校验。 */
  'plugin-install': (el) => installRecommendedPlugin(el),
  'plugin-open-repo': (el) => openExternal(el.dataset.url),

  /* ---- 日志 ---- */
  'log-pause': () => togglePauseLogs(),
  'log-clear': () => clearLogs(),
  'log-export': () => exportLogs(),

  /* ---- WebUI ---- */
  'webui-open-external': () => openExternal('http://127.0.0.1:6099'),

  /* ---- 设置 ---- */
  'detect-napcat': () => detectPath('napcat'),
  'detect-qq': () => detectPath('qq'),
  'browse-napcat': () => browsePath('napcat'),
  'browse-qq': () => browsePath('qq'),
  'toggle-setting': (el) => toggleSetting(el.dataset.key, el),
  /* change 型（见 CHANGE_ACTIONS）：el 是控件本身，直接读 el.value */
  'setting-select': (el) => saveSelectSetting(el.dataset.key, el),
  'setting-proxy': (el) => saveProxy(el),
};

/* change 型 action：只在 change 事件里派发，click 委托必须跳过它们——
 * 否则点开下拉的瞬间就会先用旧值写一次盘（select 的 click 早于 change）。 */
const CHANGE_ACTIONS = new Set(['setting-select', 'setting-proxy']);

/* 全局 click 委托：任何带 data-action 的元素被点中时，按 action 名调 handler。 */
document.addEventListener('click', (e) => {
  const el = e.target.closest('[data-action]');
  if(!el) return;
  const action = el.dataset.action;
  if(CHANGE_ACTIONS.has(action)) return;
  const fn = actions[action];
  if(typeof fn === 'function'){ fn(el); }
  else { console.warn('未注册的 action:', action); }
});
/* 全局 change 委托：只接 CHANGE_ACTIONS 里的 action（下拉/输入框这类"改完才生效"的控件）。
 * el 就是控件本身，handler 直接读 el.value；不再需要往元素上写内联的 change 处理器。 */
document.addEventListener('change', (e) => {
  const el = e.target.closest('[data-action]');
  if(!el) return;
  const action = el.dataset.action;
  if(!CHANGE_ACTIONS.has(action)) return;
  const fn = actions[action];
  if(typeof fn === 'function'){ fn(el); }
  else { console.warn('未注册的 action:', action); }
});

/* 窗口控制按钮（标题栏）也走统一系统，不再单独 bind */

/* 在系统默认程序打开目录或 URL（后端 open_external）。 */
async function openExternal(path){
  await callCmd('open_external', {path});
}
/* 打开插件目录：从配置读 plugins_dir，再调 open_external。 */
async function openPluginsDir(){
  const dir = state.cfg && state.cfg.plugins_dir;
  if(!dir){ toast('插件目录未配置，请先在设置页配置', 'warn'); return; }
  await openExternal(dir);
}
/* 一键安装推荐插件：把 url + 目录名交给后端下载解压。
 * 后端负责域名白名单、目录名白名单、单层目录展平，并复用拖放安装那套解压逻辑。
 * 这里只负责 UI：禁用整张卡片（防重复点）、显示"安装中"、装完刷新列表。
 * 注意后端那个 command 是 async 的（下载最长几分钟），所以界面不会被卡住。 */
async function installRecommendedPlugin(el){
  const name = el.dataset.name || el.dataset.id || '';
  const url = el.dataset.url || '';
  if(!url){ toast('该插件没有提供可直接安装的包，请打开仓库按说明构建', 'warn'); return; }
  const card = el.closest('.plugin-cell');
  if(card) card.classList.add('pl-installing');
  const oldText = el.textContent;
  el.textContent = '安装中…';
  toast(`正在下载 ${name}…`);
  try{
    const r = await callCmd('install_plugin_from_url', {url, name});
    if(r){
      toast(r);
      await reloadPlugins();
      render();
      return;
    }
    // 失败时 callCmd 已经把后端错误串 toast 出来了（里面包含"配代理 / 手动下载"的建议）
  } finally {
    // render() 可能已经重建了卡片、el 脱离文档；仍还原文案，避免节点被复用时残留"安装中…"
    el.textContent = oldText;
    if(card) card.classList.remove('pl-installing');
  }
}
/* 导出日志：把内存里的日志拼成文本，调 export_logs 落盘，toast 显示路径。 */
async function exportLogs(){
  if(!state.logs.length){ toast('暂无日志可导出', 'warn'); return; }
  const text = state.logs.map(l=>`[${l.ts}] [${safeLevel(l.lvl).toUpperCase()}] ${l.msg}`).join('\n');
  const p = await callCmd('export_logs', {content: text});
  if(p) toast('日志已导出: ' + p);
}
/* 在线时长：基于 state.startedAt 在前端本地格式化；停止时返回占位符。 */
function uptime(){
  // 停止时给一个中性的占位符，不再显示 "--:--:--"（既长又像坏掉的计时器）；
  // 卡片副标题会同步显示"未运行"，语义由副标题承担。
  if(!state.running) return '—';
  const s = Math.floor((Date.now()-state.startedAt)/1000);
  const h = String(Math.floor(s/3600)).padStart(2,'0');
  const m = String(Math.floor(s%3600/60)).padStart(2,'0');
  const sec = String(s%60).padStart(2,'0');
  return `${h}:${m}:${sec}`;
}
/* 生成"运行中/已停止"状态徽章的 HTML 片段（纯静态标签，无用户数据） */
function statusPill(on){
  if(on) return `<span class="status-pill ok"><span class="dot ok"></span>运行中</span>`;
  return `<span class="status-pill off"><span class="dot off"></span>已停止</span>`;
}
