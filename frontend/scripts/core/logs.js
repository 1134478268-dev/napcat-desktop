/* 日志视图渲染、过滤、暂停与清空
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ================= 日志 ================= */
/* 日志级别白名单。level 来自 napcat://log 事件的 payload，属于外部数据；
 * 它会被拼进 class 属性（class="log-line ${lvl}"），不做白名单就等于让外部字符串
 * 参与拼 HTML。后端目前只发固定几个值，仍然白名单化——未知/畸形值一律降级成 info。 */
const LOG_LEVELS = ['info','ok','warn','err','recv','plugin'];
function safeLevel(lvl){
  const v = String(lvl==null ? '' : lvl).toLowerCase();
  return LOG_LEVELS.includes(v) ? v : 'info';
}
/* 日志来源白名单：napcat / ai / bot。同样来自事件 payload，参与拼 HTML，必须白名单化。 */
const LOG_SOURCES = ['napcat','ai','bot'];
const SOURCE_LABEL = {napcat:'NapCat', ai:'AI', bot:'机器人'};
function safeSource(s){
  const v = String(s==null ? '' : s).toLowerCase();
  return LOG_SOURCES.includes(v) ? v : 'napcat';
}
/* 日志渲染：按级别 + 来源 + 关键字过滤 state.logs，逐行 escapeHtml——
 * 为什么必须转义：RECV 级别是群/私聊消息原文，消息体可能含 `<img onerror=...>`，
 * 不转义等于把别人发的消息当 HTML 在桌面端执行（存储型 XSS）。
 * 自动滚到底（tail 行为）：事件驱动的增量重绘已被"暂停"挡在门外（见 boot 里的 napcat://log 监听），
 *   所以能走到这里的渲染都是用户主动触发的（切到日志页/改过滤/清空），此时滚到底才是预期。
 * 暂停期间日志照旧进 state.logs，只是不重绘、不自动滚动，用户往回翻时不会被拽下去。 */
function renderLogs(){
  const body = $('#log-body'); if(!body) return;
  const lv = $('#log-level') ? $('#log-level').value : '';
  const src = $('#log-source') ? $('#log-source').value : '';
  const kw = ($('#log-filter') ? $('#log-filter').value : '').toLowerCase();
  const lines = state.logs.filter(l=>
    (!lv || safeLevel(l.lvl)===lv) &&
    (!src || safeSource(l.src)===src) &&
    (!kw || l.msg.toLowerCase().includes(kw))
  );
  body.innerHTML = lines.map(l=>{
    const lvl = safeLevel(l.lvl);
    const s = safeSource(l.src);
    return `<div class="log-line ${lvl}"><span class="ts">${escapeHtml(l.ts)}</span><span class="src src-${s}">${SOURCE_LABEL[s]}</span><span class="lvl">${lvl.toUpperCase()}</span><span class="msg">${escapeHtml(l.msg)}</span></div>`;
  }).join('');
  body.scrollTop = body.scrollHeight;
}
/* 概览页"最近日志"面板（render() 与 napcat://log 事件共用这一个实现）：
 * 取最后 30 条、倒序（新的在上），每行是"时间 + 级别标签 + 正文"，按级别着色。
 * 为什么从 8 条提到 30 条：日志区改成自适应高度后 8 条填不满会显得很空，
 * 而它现在是内部滚动区，条数多一点才配得上"最近日志"。
 * 着色走 CSS 类（.ov-log.<level>）而不是内联 style；级别先过 safeLevel 白名单，
 * 所以 class 属性里永远不会混进事件 payload 带来的意外字符串。 */
function renderOverviewLogs(){
  const el = $('#mini-log'); if(!el) return;
  const lines = state.logs.slice(-30).reverse();
  el.innerHTML = lines.length ? lines.map(l=>{
    const lvl = safeLevel(l.lvl);
    return `<div class="ov-log ${lvl}"><span class="ov-log-ts">${escapeHtml(l.ts)}</span><span class="ov-log-lv">${lvl.toUpperCase()}</span><span class="ov-log-msg">${escapeHtml(l.msg)}</span></div>`;
  }).join('') : '<div class="ov-log-empty">暂无日志，启动 NapCat 后此处会实时显示</div>';
}
/* 暂停按钮文案：暂停期间顺带显示积压条数，让用户知道日志没丢只是在攒着 */
function pauseLabel(){
  if(!state.pausedLogs) return '暂停';
  return state.pausedCount ? `继续滚动 (${state.pausedCount})` : '继续滚动';
}
/* 暂停按钮文字就地刷新（不整体 render，避免打断用户正在看的滚动位置）。
 * pushLog 每行调一次，但只写一个 textContent，比 innerHTML 重绘便宜得多。 */
function updatePauseBtn(){
  const btn = document.getElementById('btn-pause');
  if(btn) btn.textContent = pauseLabel();
}
/* 清空：只清前端缓冲，不动后端日志文件；暂停开关不清历史。
 * 两个渲染函数各自在自己的容器不存在时直接 return，所以这里可以无脑一起调。 */
function clearLogs(){
  state.logs = [];
  state.pausedCount = 0;
  updatePauseBtn();
  renderLogs();
  renderOverviewLogs();
  toast('日志已清空');
}
/* 暂停/继续：暂停的是"自动增量重绘 + 自动滚动"，不是日志接收——
 * pushLog 始终照收（200 条上限照旧），所以恢复时能看到暂停期间积压的日志，一条不丢。 */
function togglePauseLogs(){
  state.pausedLogs = !state.pausedLogs;
  state.pausedCount = 0;
  updatePauseBtn();
  if(state.pausedLogs){
    toast('已暂停自动滚动（日志仍在后台接收）');
  } else {
    // 恢复时立即补一次渲染，把暂停期间攒下的日志显示出来
    if(location.hash.includes('#/logs')) renderLogs();
    if(location.hash.includes('#/overview')) renderOverviewLogs();
    toast('已恢复自动滚动');
  }
}
