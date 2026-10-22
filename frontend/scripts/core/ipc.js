/* Tauri IPC 统一入口与配置落盘
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ================= Tauri IPC 统一入口 =================
 * 浏览器里直接打开 index.html 调试时 window.__TAURI__ 不存在，
 * 这里兜底 toast 而不是抛 undefined；后端 reject 的错误串一律 toast（textContent，安全）。 */
async function callCmd(cmd, args){
  if(!window.__TAURI__){ toast('未在 Tauri 环境运行（浏览器调试模式）', 'warn'); return null; }
  try{
    return await window.__TAURI__.core.invoke(cmd, args);
  }catch(e){
    toast('操作失败：' + String(e), 'warn');
    return null;
  }
}

/* load_config 返回的 AppConfig → 前端 state 显示模型的映射。
 * Account {qq,nick,enabled,saved_login} → state.accounts {qq,nick,enabled,saved,color}；
 *   enabled 是"启动时是否拉起该号"的持久配置，与"正在运行"（state.runningQq）严格分开。
 *   saved_login 后端目前只声明、从不写入，仅作为"这个号成功登录过"的展示标记（见 applyStatus）。
 * OneBotService {type,port,addr,memo,enabled} → state.services {...,running}。 */
function hydrateConfig(cfg){
  state.cfg = cfg;
  state.accounts = (cfg.accounts||[]).map(a=>{
    const hash = [...String(a.qq)].reduce((c,ch)=>c+ch.charCodeAt(0),0);
    /* qq 统一强转成字符串：账号卡片用 a.qq.slice(-2) 取后缀、日志/状态里又要和
     * status.qq 严格比较，若哪天后端把 qq 序列化成数字，slice 会直接抛错、
     * 字符串比较也会悄悄不相等。入口处归一，后面就不必处处 String()。 */
    return { qq:String(a.qq), nick:a.nick||a.qq, enabled:a.enabled !== false, saved:!!a.saved_login,
             color: AVATAR_GRADIENTS[hash % AVATAR_GRADIENTS.length] };
  });
  state.services = (cfg.onebot||[]).map(s=>({
    type:s.type, port:s.port, addr:s.addr||'', memo:s.memo||'', token:s.token||'',
    enabled:s.enabled !== false, running:state.running,
  }));
}

/* 调 list_plugins({ path }) 重建 state.plugins。
 * PluginInfo {dir,name,version,desc,author,enabled,has_exec}；
 * size/updated 后端不提供，用占位符避免渲染 undefined。 */
async function reloadPlugins(){
  const dir = state.cfg && state.cfg.plugins_dir;
  if(!dir){ state.plugins = []; return; }
  const list = await callCmd('list_plugins', {path: dir});
  if(!Array.isArray(list)) return;
  state.plugins = list.map(p=>({
    dir:p.dir, name:p.name, version:p.version||'-', desc:p.desc||'', author:p.author||'-',
    on:!!p.enabled, size:'—', updated:'—',
  }));
}

/* save_config：后端要求整对象覆盖写（不是 patch）。
 * 先把 UI 层 state.accounts/services 回写进 cfg 再整体 invoke。
 * 写 enabled 时只认持久配置 a.enabled —— 绝不能拿"是否在运行"来填这个字段，
 * 否则启动 NapCat 后随便触发一次保存（改备注/改设置/加账号）就会把其它账号永久禁用。 */
async function saveConfig(){
  if(!state.cfg){ toast('配置尚未加载', 'warn'); return; }
  state.cfg.accounts = state.accounts.map(a=>({qq:a.qq, nick:a.nick, enabled:a.enabled !== false, saved_login:a.saved}));
  state.cfg.onebot = state.services.map(s=>({type:s.type, port:s.port, addr:s.addr, memo:s.memo, token:s.token||'', enabled:s.enabled!==false}));
  await callCmd('save_config', {config: state.cfg});
}

/* 插件备注：按插件目录名存到 cfg.plugin_memos，失焦自动保存。 */
async function savePluginMemo(dir, value){
  if(!state.cfg) return;
  if(!state.cfg.plugin_memos) state.cfg.plugin_memos = {};
  state.cfg.plugin_memos[dir] = value.trim();
  await saveConfig();
  toast('插件备注已保存');
}

/* 设置页开关：切样式 → 写 cfg 布尔 → save_config */
async function toggleSetting(key, el){
  el.classList.toggle('on');
  if(state.cfg){ state.cfg[key] = el.classList.contains('on'); await saveConfig(); toast('设置已保存'); }
}
/* 设置页下拉（max_restarts 是 number，option value 是字符串需转换） */
async function saveSelectSetting(key, sel){
  if(state.cfg){ state.cfg[key] = Number(sel.value); await saveConfig(); toast('设置已保存'); }
}
/* 代理输入框：失焦/回车时保存 */
async function saveProxy(inp){
  if(state.cfg){ state.cfg.proxy = inp.value.trim(); await saveConfig(); toast('代理已保存'); }
}
