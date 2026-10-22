/* DOM 与展示工具（$ / escapeHtml / toast）
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ================= Helpers ================= */
/* 极简选择器简写 */
const $ = s => document.querySelector(s);
/* HTML 转义：把 & < > " ' 转成实体，防止用户可控字符串被当标签解析。
 * 注意：放进 HTML 属性（如 value="..."）也必须转，" 转 &quot; 才能闭合安全。 */
function escapeHtml(s){
  return String(s).replace(/[&<>"']/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
/* 全局底部轻提示；type='warn' 警示色（失败/警告）、type='ok' 成功色，其余用默认强调色。
 * 2s 自动消失。注意：只识别这两个 type，传别的值等价于不传——所以调用点不要写 'info' 之类的臆造类型。 */
function toast(t, type){
  const el = $('#toast'); el.textContent = t;
  const color = type==='warn' ? 'var(--warn)' : type==='ok' ? 'var(--ok)' : '';
  el.style.borderColor = color;
  el.style.color = color;
  el.classList.add('show');
  /* 警告/错误给 6 秒，成功提示保持 2 秒。
   * 为什么区分：一条网络类失败信息可能很长（"下载失败（curl 退出码 28）：…；
   * 请配置代理后重试，或手动下载后拖放安装"），2 秒根本读不完，而这恰恰是最需要看清的内容。 */
  const ms = type==='warn' ? 6000 : 2000;
  clearTimeout(toast._t); toast._t = setTimeout(()=>{el.classList.remove('show');el.style.borderColor='';el.style.color='';},ms);
}
