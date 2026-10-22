/* 交互逻辑：账号增删、网络配置表单、主题切换、插件备注、路径检测
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ================= Actions（所有按钮的事件处理函数） ================= */
/* 在线时长秒表：只在概览页跑，切走就停，避免后台空转 */
let uptimeTimer=null;
function startUptime(){
  stopUptime();
  uptimeTimer = setInterval(()=>{
    const el=$('#stat-uptime'); if(el) el.textContent = uptime();
  },1000);
}
function stopUptime(){ if(uptimeTimer){clearInterval(uptimeTimer);uptimeTimer=null;} }

/* 启动/停止 NapCat 主进程（概览页大按钮）。
 * 调的命令：start_napcat（不传 qq，后端自动选第一个 enabled 账号）/ stop_napcat。
 * 成功后不在这里改 UI——napcat://status 事件会回流统一更新（避免双击竞态下状态错乱）；
 * 失败（launcher.bat 缺失/路径未配置等）callCmd 已 toast 后端错误串。
 * 注意：这里的"启停"是概览页大按钮，直接由 actions['start'] / actions['stop'] 调 callCmd；
 * 不再包一层 toggleRun(on) ——那个函数只有定义没有任何调用点。 */

/* 单个账号的启动/停止。
 * 边界：后端是单主进程模型，没有 per-account stop——
 *   启动指定号 = start_napcat({ qq })（后端校验 qq 必须 5-11 位纯数字）；
 *   停止任意号 = stop_napcat() 停掉整个主进程。
 * "这个号是否在跑"由 runningQq（进程事实）推导，不能读持久字段 a.enabled，
 * 否则禁用中的账号一启动就会显示成"已停止"，按钮语义也跟着错。 */
async function toggleAcct(qq){
  const a = state.accounts.find(x=>x.qq===qq);
  if(!a) return;
  if(state.running && state.runningQq === qq){ await callCmd('stop_napcat'); }
  else { await callCmd('start_napcat', {qq}); }
}
/* 切换账号的"启用"开关：只改 enabled（= 下次启动 NapCat 时带不带这个号），
 * 不影响已经在跑的进程。这个开关是 enabled 唯一的编辑入口——早先它没有 UI，
 * 只被 applyStatus 当成"正在运行"的副产品写来写去，才导致配置被污染。
 * 每次改动立刻 save_config 整体覆盖写回（后端不支持增量 patch），并 render 回填开关态。 */
async function toggleAcctEnabled(qq){
  const a = state.accounts.find(x=>x.qq===qq);
  if(!a) return;
  a.enabled = a.enabled === false;   // 取反：false/未定义 → true，true → false
  await saveConfig();
  toast(a.enabled
    ? `已启用账号 ${qq}，启动时会拉起该号`
    : `已禁用账号 ${qq}，启动时不带该号`);
  render();
}
/* 重启：先 stop 再 start。中间留 500ms 让 watcher 退出，
 * 否则立刻 start 会撞后端"NapCat 已在运行"校验。
 * 保留成独立函数（而不是塞进 actions 表的内联闭包），这样"停 → 等 → 起"只有一份实现。 */
async function restartNapCat(){
  await callCmd('stop_napcat');
  await new Promise(r=>setTimeout(r,500));
  await callCmd('start_napcat');
}
/* napcat://status 事件 + get_napcat_status 返回值统一入口。
 * payload(StatusInfo)：{ running, pid, qq, last_exit_code, restart_count, message }
 * crashed 判定：已停止且退出码非空非 0（异常崩溃待恢复）；exit_code=0/null 视为正常停止。 */
function applyStatus(s){
  if(!s) return;
  state.running = !!s.running;
  // crashed 判定：优先看后端 message 语义，而不是只看退出码。
  //   - "正常退出" / "主动停止" / "未运行" → 不算崩溃
  //   - "异常退出" / "自动重启" / "崩溃" → 崩溃
  //   - 兜底：退出码非 0 视为异常
  const msg = (s.message || '').toString();
  const graceful = /正常退出|主动停止|未运行|已通过桌面端/.test(msg);
  const abnormal = /异常退出|自动重启|崩溃|异常/.test(msg);
  if(state.running){
    state.crashed = false;
  } else if(graceful){
    state.crashed = false;
  } else if(abnormal){
    state.crashed = true;
  } else {
    state.crashed = !!(s.last_exit_code !== null && s.last_exit_code !== undefined && s.last_exit_code !== 0);
  }
  // 只有 running=true 才打时间戳；停止/崩溃时清掉，避免下次启动接着上次的时间走
  if(state.running){ state.startedAt = Date.now(); }
  else { state.startedAt = null; }
  /* 只记录"当前在跑哪个号"，绝不回写 accounts：
   * 早先这里把 a.on 直接按 running+qq 覆盖，而 a.on 又是 saveConfig 写 enabled 的来源，
   * 结果 NapCat 一跑起来，除当前号外的账号全被写成 enabled:false 永久禁用
   * （复现：启用 A、B → 启动 A → 改一下 B 的备注 → 重启面板，B 变成"禁用"）。
   * 运行态是瞬时事实（runningQq），启用态是持久配置（enabled），两者从此分开。
   * 注意：running=true 只说明"子进程起来了"，后端在 spawn 成功后立刻 emit 这个状态，
   * 此刻用户可能还没扫码，所以这里绝不能顺手点亮 saved（"已记住登录态"）——
   * 那件事只认日志里的登录成功，见 boot() 中的 napcat://log 成功分支。 */
  state.runningQq = state.running ? String(s.qq || '') : '';
  $('#tray-dot').className = 'dot ' + (state.running ? 'ok' : (state.crashed ? 'err' : 'off'));
  $('#tray-text').textContent = state.running ? 'NapCat 运行中' : (state.crashed ? 'NapCat 已崩溃' : 'NapCat 已停止');
  // OneBot 服务在线徽章跟随主进程
  state.services.forEach(sv=>sv.running = state.running);
  render();
}
/* 追加一条日志并裁剪到 200 条上限（防长时间运行撑爆 DOM）。
 * 暂停期间照收不误（见 togglePauseLogs 的语义说明），只额外记一个积压计数。
 * lvl 在这里就过一遍 safeLevel 归一化：日志级别下拉过滤和渲染配色都以归一化后的值为准。 */
function pushLog(lvl,msg,src){
  state.logs.push({ts:new Date().toLocaleTimeString('zh-CN',{hour12:false}),lvl:safeLevel(lvl),msg,src:safeSource(src)});
  if(state.logs.length>200) state.logs.shift();
  if(state.pausedLogs){ state.pausedCount++; updatePauseBtn(); }
}

/* 自动检测 NapCat 安装路径。
 * 调的命令：detect_napcat_path() -> string[]（后端只扫固定安装根目录，
 * 不扫桌面/Downloads，防止执行伪造的 launcher.bat——安全审计 H1 的约定）。
 * 返回候选非空：填第一个 + 写回 cfg.napcat_path 并 save_config 持久化；
 * 空数组：保留警示提示让用户手动「浏览」（本期后端无文件选择命令）。
 * 注意：status 用 textContent 写入，候选路径来自后端，不做 innerHTML。 */
async function detectPath(key, silent){
  const status = document.getElementById('status-'+key);
  const btn = document.getElementById('btn-detect-'+key);
  const input = document.getElementById('path-'+key);
  if(!status) return;
  if(btn) btn.disabled = true;
  status.className = 'path-status loading';
  status.textContent = '正在扫描常见位置…';
  const cmd = key === 'qq' ? 'detect_qq_path' : 'detect_napcat_path';
  const cands = await callCmd(cmd);
  // 等待期间用户可能已切走路由，DOM 已失效，直接放弃后续写回
  if(!document.body.contains(status)) return;
  if(btn) btn.disabled = false;
  if(Array.isArray(cands) && cands.length){
    input.value = cands[0];
    if(state.cfg){
      if(key === 'qq'){
        state.cfg.qq_path = cands[0];
      } else {
        state.cfg.napcat_path = cands[0];
        // 插件目录由后端 resolve_plugins_dir 自动推断（shell\plugins 或 plugins）
        // 这里先清空，下面 save_config 后重新 load_config 让后端修正
        state.cfg.plugins_dir = '';
      }
      await callCmd('save_config', {config: state.cfg});
      // 重新 load_config 拿到后端修正后的 plugins_dir
      if(key !== 'qq'){
        const fresh = await callCmd('load_config');
        if(fresh){
          state.cfg = fresh;
          const pluginsInput = document.getElementById('path-plugins');
          if(pluginsInput) pluginsInput.value = fresh.plugins_dir || '';
        }
      }
    }
    status.className = 'path-status ok';
    status.textContent = '✓ 已自动检测到：' + cands[0];
    if(!silent) toast('路径已自动填入');
  } else {
    status.className = 'path-status warn';
    status.textContent = '⚠ 未在常见位置找到，请点「浏览」手动选择目录';
    if(!silent) toast('未自动找到，请手动选择', 'warn');
  }
}

/* 弹系统目录选择对话框，用户选完后写回输入框和配置。 */
async function browsePath(key){
  const input = document.getElementById('path-'+key);
  const status = document.getElementById('status-'+key);
  const current = input ? input.value : '';
  const picked = await callCmd('pick_directory', {defaultPath: current || 'D:\\'});
  if(!picked){ return; } // 用户取消
  if(input) input.value = picked;
  if(state.cfg){
    if(key === 'qq') state.cfg.qq_path = picked;
    else {
      state.cfg.napcat_path = picked;
      state.cfg.plugins_dir = '';
    }
    await callCmd('save_config', {config: state.cfg});
    if(key !== 'qq'){
      const fresh = await callCmd('load_config');
      if(fresh){
        state.cfg = fresh;
        const pluginsInput = document.getElementById('path-plugins');
        if(pluginsInput) pluginsInput.value = fresh.plugins_dir || '';
      }
    }
  }
  if(status){ status.className = 'path-status ok'; status.textContent = '✓ 已选择：' + picked; }
  toast('已选择目录：' + picked);
}

/* 随机造日志的定时器已删除：真实日志由 napcat://log 事件逐行推送（注册见 boot()）。 */

/* ================= Bind（导航项、hash 路由、日志过滤输入的事件绑定） ================= */
/* 导航项绑定：点击/回车/空格都切路由；role+tabindex 保证键盘可达 */
document.querySelectorAll('.nav-item').forEach(n=>{
  n.setAttribute('role','button');
  n.setAttribute('tabindex','0');
  n.addEventListener('click',()=>go(n.dataset.route));
  n.addEventListener('keydown',e=>{ if(e.key==='Enter'||e.key===' '){e.preventDefault();go(n.dataset.route);} });
});
window.addEventListener('hashchange', render);
/* 日志过滤框：input 事件实时过滤（日志量小，每敲一个字重渲染没问题） */
document.addEventListener('input', e=>{ if(e.target.id==='log-filter') renderLogs(); });
document.addEventListener('change', e=>{ if(e.target.id==='log-level'||e.target.id==='log-source') renderLogs(); });

/* ================= Theme（明暗双主题，逻辑保持不动） =================
 * 主题存 localStorage，首屏 initTheme 恢复；切换靠 <html data-theme> 换 CSS 变量。 */
const ICON_SUN = '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>';
const ICON_MOON = '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>';
function setTheme(t){
  document.documentElement.setAttribute('data-theme', t);
  try{ localStorage.setItem('napcat-theme', t); }catch(e){}
  document.getElementById('theme-btn').innerHTML = (t==='dark' ? ICON_SUN : ICON_MOON);
}
function toggleTheme(){
  const cur = document.documentElement.getAttribute('data-theme')==='light' ? 'light' : 'dark';
  setTheme(cur==='dark' ? 'light' : 'dark');
}
(function initTheme(){
  let t='dark';
  try{ t = localStorage.getItem('napcat-theme') || 'dark'; }catch(e){}
  setTheme(t);
})();

/* ================= Plugin Actions ================= */
/* 为什么这里没有"插件开关切换"函数：
 * 后端没有 toggle 命令，而 reloadPlugins() 每次都从磁盘重读，前端翻一下 p.on 再 render
 * 只会造成"点了禁用、刷新又变启用"的假象（旧代码那句"刷新后持久化"是虚假承诺）。
 * 也不要把禁用实现成给目录加 .disabled 后缀——那个约定只是本面板自己发明的识别规则，
 * NapCat 侧并不会因此停止加载插件（官方插件启停由 NapCat 的 ctx.pluginManager /
 * WebUI 插件市场负责：https://doc.napneko.icu/develop/plugin/index ），
 * 用一个假功能换另一个假功能更糟。所以卡片上只保留只读状态标签。 */
/* 刷新插件列表。
 * 调的命令：list_plugins({ path: cfg.plugins_dir }) -> PluginInfo[]。
 * 返回后重建 state.plugins（size/updated 后端不提供，用占位符）再 render()。 */
async function refreshPlugins(){
  toast('正在扫描 plugins/ 目录…');
  await reloadPlugins();
  toast(`已识别 ${state.plugins.length} 个插件`);
  render();
}

/* ================= Memo / Nick editing（行内编辑，不弹框） ================= */
/* 把备注替换成输入框；value 里 escapeHtml 防引号截断属性。 */
function editNick(i){
  const el = document.getElementById('nick-'+i);
  if(!el || el.querySelector('input')) return;
  const cur = state.accounts[i].nick;
  el.innerHTML = '';
  const inp = document.createElement('input');
  inp.className = 'nick-input';
  inp.value = cur;
  el.appendChild(inp);
  inp.focus();
  inp.select();
  let saved = false;
  const doSave = () => {
    if(saved) return;
    saved = true;
    const t = inp.value.trim();
    if(t && t !== state.accounts[i].nick){
      state.accounts[i].nick = t;
      saveConfig().then(()=>toast('备注已保存')).catch(()=>toast('保存失败','warn'));
    }
    restoreNick(i);
  };
  const doCancel = () => { restoreNick(i); };
  inp.addEventListener('blur', doSave);
  inp.addEventListener('keydown', (e)=>{
    if(e.key === 'Enter'){ e.preventDefault(); inp.blur(); }
    else if(e.key === 'Escape'){ e.preventDefault(); doCancel(); }
  });
}
/* 把输入框还原成"备注 + 编辑图标"。保存/取消都走这里。 */
function restoreNick(i){
  const el = document.getElementById('nick-'+i);
  if(!el) return;
  el.innerHTML = escapeHtml(state.accounts[i].nick) +
    '<span class="edit-icon" data-action="acct-edit-nick" data-i="'+i+'" title="编辑备注">'+
    '<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/></svg></span>';
}

/* 添加账号：弹输入框收集 QQ 号和备注。
 * 前端先做格式校验（5-11 位纯数字），和后端 start_napcat 的校验一致，
 * 避免用户输错还要等 IPC 回来。
 * 安全：QQ 号只允许数字，备注最后进 DOM 时由 escapeHtml 转义。 */
/* 自定义模态弹窗：替代原生 prompt()。
 * 返回 Promise<string|null>，确定 resolve 输入值，取消 resolve null。 */
function showModal(title, placeholder, defaultValue){
  return new Promise(resolve=>{
    const overlay = document.getElementById('modal-overlay');
    const titleEl = document.getElementById('modal-title');
    const input = document.getElementById('modal-input');
    const ok = document.getElementById('modal-ok');
    const cancel = document.getElementById('modal-cancel');
    titleEl.textContent = title;
    input.placeholder = placeholder || '';
    input.value = defaultValue || '';
    overlay.classList.add('show');
    setTimeout(()=>input.focus(), 50);
    const cleanup = (val)=>{
      overlay.classList.remove('show');
      ok.removeEventListener('click', onOk);
      cancel.removeEventListener('click', onCancel);
      input.removeEventListener('keydown', onKey);
      resolve(val);
    };
    const onOk = ()=>cleanup(input.value.trim());
    const onCancel = ()=>cleanup(null);
    const onKey = (e)=>{
      if(e.key === 'Enter') cleanup(input.value.trim());
      if(e.key === 'Escape') cleanup(null);
    };
    ok.addEventListener('click', onOk);
    cancel.addEventListener('click', onCancel);
    input.addEventListener('keydown', onKey);
  });
}

async function addAccount(){
  const qq = (await showModal('添加 QQ 账号','输入 5-11 位数字 QQ 号','') || '').trim();
  if(!qq) return;
  if(!/^\d{5,11}$/.test(qq)){ toast('QQ号必须是 5-11 位纯数字', 'warn'); return; }
  if(state.accounts.some(a=>a.qq===qq)){ toast('该 QQ 号已在列表中', 'warn'); return; }
  const hash = [...qq].reduce((c,ch)=>c+ch.charCodeAt(0),0);
  state.accounts.push({
    qq, nick: qq, enabled: true, saved: false,
    color: AVATAR_GRADIENTS[hash % AVATAR_GRADIENTS.length],
  });
  await saveConfig();
  toast('已添加账号 '+qq+'，可在卡片上点编辑图标改备注');
  render();
}

/* 删除账号：从本地配置移除，不删磁盘上的 NapCat 配置文件（用户可能还要用）。
 * 如果删的正是当前在跑的那个号，先停主进程（后端单进程模型，停就是全停）。 */
async function deleteAccount(qq){
  if(!confirm('确定删除账号 '+qq+' 吗？\n（只从桌面端配置移除，不删除 NapCat 配置文件）')) return;
  if(state.running && state.runningQq === qq){ await callCmd('stop_napcat'); }
  state.accounts = state.accounts.filter(x=>x.qq!==qq);
  await saveConfig();
  toast('已删除账号 '+qq);
  render();
}

/* OneBot 服务内联表单 HTML。form 对象由 state.netForm 提供。
 * 反向 WS 时地址字段必填；HTTP/WS 时地址留空（监听 0.0.0.0）。 */
function netFormCard(form){
  const isPost = form.type === '反向WS';
  return `
  <div class="net-form" id="net-form">
    <label>服务类型
      <select id="nf-type" data-nf="type">
        <option value="HTTP" ${form.type==='HTTP'?'selected':''}>HTTP 服务端</option>
        <option value="WS" ${form.type==='WS'?'selected':''}>WebSocket 服务端（正向）</option>
        <option value="反向WS" ${form.type==='反向WS'?'selected':''}>WebSocket 客户端（反向）</option>
      </select>
    </label>
    <label>端口
      <input id="nf-port" data-nf="port" type="number" min="1" max="65535" value="${escapeHtml(String(form.port||''))}" placeholder="如 3000">
    </label>
    <label class="${isPost?'':'full'}">${isPost?'反向连接地址':'访问地址（留空=监听所有网卡）'}
      <input id="nf-addr" data-nf="addr" value="${escapeHtml(form.addr||'')}" placeholder="${isPost?'wss://your-server/ws':'http://127.0.0.1:3000'}">
    </label>
    <label>Token（可选）
      <input id="nf-token" data-nf="token" value="${escapeHtml(form.token||'')}" placeholder="鉴权 token">
    </label>
    <label class="full">备注
      <input id="nf-memo" data-nf="memo" value="${escapeHtml(form.memo||'')}" placeholder="这个服务用来做什么？">
    </label>
    <div class="net-form-actions">
      <button class="btn sm ghost" data-action="net-cancel">取消</button>
      <button class="btn sm primary" data-action="net-save">保存服务</button>
    </div>
  </div>`;
}

/* 从 DOM 读表单字段到 state.netForm */
function readNetForm(){
  const f = state.netForm;
  if(!f) return;
  document.querySelectorAll('#net-form [data-nf]').forEach(el=>{
    f[el.dataset.nf] = el.value.trim();
  });
  f.port = parseInt(f.port, 10);
}

/* 新建：打开空表单 */
function openNewNet(){
  state.netForm = { i:null, editing:false, type:'HTTP', port:3000, addr:'', token:'', memo:'' };
  render();
}
/* 编辑：用现有数据填表单 */
function openEditNet(i){
  const s = state.services[i];
  state.netForm = { i, editing:true, type:s.type, port:s.port, addr:s.addr||'', token:s.token||'', memo:s.memo||'' };
  render();
}
/* 取消 */
function cancelNet(){
  state.netForm = null;
  render();
}
/* 保存：校验后写入 state.services，再 saveConfig */
async function saveNet(){
  readNetForm();
  const f = state.netForm;
  if(!f.port || f.port < 1 || f.port > 65535){ toast('端口必须是 1-65535', 'warn'); return; }
  if(f.type === '反向WS' && !f.addr){ toast('反向 WS 必须填连接地址', 'warn'); return; }
  const svc = { type:f.type, port:f.port, addr:f.addr, token:f.token, memo:f.memo, enabled:true, running:false };
  if(f.editing && f.i !== null){
    // 保留原 enabled/running
    svc.enabled = state.services[f.i].enabled !== false;
    svc.running = state.services[f.i].running;
    state.services[f.i] = svc;
  } else {
    state.services.push(svc);
  }
  state.netForm = null;
  await saveConfig();
  toast('OneBot 服务已保存');
  render();
}
/* 删除服务 */
async function deleteNet(i){
  const s = state.services[i];
  if(!confirm(`确认删除 ${s.type}:${s.port} ${s.memo||''}？`)) return;
  state.services.splice(i, 1);
  await saveConfig();
  toast('已删除');
  render();
}
/* 启用/禁用开关 */
async function toggleNet(i){
  state.services[i].enabled = state.services[i].enabled === false;
  await saveConfig();
  render();
}
