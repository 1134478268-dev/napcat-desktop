/* hash 路由与页面重绘
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ================= Router（hash 路由：页面切换不刷新） ================= */
const PAGE_NAMES = {overview:'概览',accounts:'账号管理',network:'OneBot 网络',plugins:'插件',logs:'实时日志',webui:'内嵌 WebUI',settings:'设置'};
/* 切路由：只改 hash，真正渲染交给 hashchange -> render() */
function go(route){
  location.hash = '#/'+route;
}
/* 渲染当前路由视图；按视图挂副作用：
 * logs→立即刷日志、overview→刷最近日志面板+启动秒表、settings→路径自动检测兜底 */
function render(){
  const route = (location.hash.replace('#/','')||'overview');
  const viewEl = $('#view');
  /* 概览页用 .view-fill（flex 纵列）让日志卡吃掉剩余高度；其余页面保持原来的块级布局。
   * 每次 render 都整体重置 className，避免从概览切走时残留 flex 布局影响其它页面。 */
  viewEl.className = route==='overview' ? 'view view-fill' : 'view';
  viewEl.innerHTML = views[route] ? views[route]() : views.overview();
  document.querySelectorAll('.nav-item').forEach(n=>{
    n.classList.toggle('active', n.dataset.route===route);
  });
  $('#crumb-page').textContent = PAGE_NAMES[route]||'概览';
  if(route==='logs') renderLogs();
  if(route==='overview') renderOverviewLogs();
  if(route==='settings'){
    /* 自动检测只在对应字段为空时兜底，绝不覆盖用户手选值：
     * detectPath 一旦拿到非空候选就会写 cfg.*_path + 清 plugins_dir + save_config，
     * 早先无条件调用导致"用户在浏览里手选的目录，每次进设置页都被改回自动检测的第一个候选"。
     * silent=true 只抑制 toast，拦不住写盘，所以这里必须用 cfg 判空来把门。 */
    const needNapcat = state.cfg && !state.cfg.napcat_path;
    const needQq = state.cfg && !state.cfg.qq_path;
    if(needNapcat || needQq){
      setTimeout(()=>{ if(needQq) detectPath('qq', true); if(needNapcat) detectPath('napcat', true); }, 200);
    }
  }
  if(route==='overview' && state.running){ startUptime(); } else { stopUptime(); }
}
