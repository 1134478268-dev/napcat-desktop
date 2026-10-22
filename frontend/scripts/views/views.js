/* 7 个视图的 HTML 渲染函数（纯字符串模板，不直接操作 DOM）
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */
/* ================= Views（每个视图一个函数，返回该页 HTML 字符串） =================
 * 约定：视图函数只读 state、拼 HTML；交互逻辑放 Actions 区，通过 onclick 挂接。
 * 凡用户/后端可控字符串一律过 escapeHtml——见下方各视图内的安全注释。 */
const views = {
  /* 概览视图：主账号卡片 + 统计卡 + 最近日志面板。
   * 日志面板只留空容器，内容统一由 renderOverviewLogs() 填充（避免视图函数和
   * render() 两处各写一套"最近日志"逻辑，长度/顺序/配色不一致）。 */
  overview(){
    /* 展示哪个账号：优先"正在运行的那个"（state.runningQq 来自后端 status，是进程事实），
     * 其次用户启用的第一个账号（enabled 是持久配置），都没有就兜底第一个。
     * 注意两者语义不同：处于禁用态的账号也可能是当前正在跑的号——禁用只影响下次启动带不带它。 */
    const runningAcct = (state.running && state.accounts.find(a=>a.qq===state.runningQq))
      || state.accounts.find(a=>a.enabled)
      || state.accounts[0]
      || {qq:'--', nick:'未配置账号', color:'linear-gradient(135deg,#888,#666)'};
    const enabledAccts = state.accounts.filter(a=>a.enabled).length;
    const totalAccts = state.accounts.length;
    const onlineSvcs = state.services.filter(s=>s.running).length;
    const totalSvcs = state.services.length;
    /* 概览要多展示"配置了多少东西"：账号数、服务数、插件数全部取自前端已有的 state，
     * 不新增后端 command。插件启用数是按目录名 .disabled 约定统计的展示值。 */
    const totalPlugins = state.plugins.length;
    const onPlugins = state.plugins.filter(p=>p.on).length;
    const napcatPath = (state.cfg && state.cfg.napcat_path) || '';
    return `
    <h2 class="view-title">概览</h2>
    <div class="hero">
      <div class="avatar" style="background:${escapeHtml(runningAcct.color||'linear-gradient(135deg,#5ab0ff,#a06bff)')}">${escapeHtml(runningAcct.nick.slice(0,1)||'Q')}</div>
      <div class="hero-id">
        <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:6px">
          <span style="font-size:20px;font-weight:700;color:var(--text-1);line-height:1.3">${escapeHtml(runningAcct.nick)}</span>
          <span style="font-size:12.5px;color:var(--text-3);font-family:var(--mono)">${escapeHtml(runningAcct.qq)}</span>
          ${state.crashed
            ? '<span class="status-pill" style="background:var(--err-soft);color:var(--err)"><span class="dot err"></span>进程已崩溃</span>'
            : statusPill(state.running)}
        </div>
        <div style="font-size:12.5px;color:var(--text-3)">
          ${state.crashed
            ? '退出码 0xC0000005 · 点下方一键拉起'
            : (state.running
              ? '运行中 · 子进程已连接'
              : '已停止 · 点右侧"启动 NapCat"开始')}
        </div>
        <div class="ov-path" title="${escapeHtml(napcatPath)}">${napcatPath ? escapeHtml(napcatPath) : '尚未配置 NapCat 路径 · 去「设置」选择'}</div>
      </div>
      <div class="hero-actions">
          ${state.crashed
            ? `<button class="btn primary" style="background:var(--warn)" data-action="wake">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:6px"><path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/></svg>一键拉起
              </button>`
            : (state.running
              ? `<button class="btn danger" data-action="stop">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:6px"><path d="M18.36 6.64a9 9 0 1 1-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/></svg>关闭 NapCat
                </button>`
              : `<button class="btn primary" data-action="start">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-2px;margin-right:6px"><path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/></svg>启动 NapCat
                </button>`)}
          ${!state.crashed ? '<button class="btn ghost" data-action="restart">重启</button>' : ''}
          <button class="btn ghost sm" data-action="simulate-crash">模拟崩溃</button>
      </div>
    </div>
    <div class="ov-stats">
      <div class="card">
        <div class="stat-label">在线时长</div>
        <div class="stat-value ${state.running?'ok':''}" id="stat-uptime">${uptime()}</div>
        <div class="stat-sub">${state.running ? '本次运行已持续' : '未运行'}</div>
      </div>
      <div class="card">
        <div class="stat-label">账号</div>
        <div class="stat-value">${enabledAccts}<span style="font-size:13px;color:var(--text-3)"> / ${totalAccts} 启用</span></div>
        <div class="stat-sub">${totalAccts===0 ? '还没添加账号' : (state.running && state.runningQq ? `正在运行 ${state.runningQq}` : '启动时使用第一个启用的账号')}</div>
      </div>
      <div class="card">
        <div class="stat-label">OneBot 服务</div>
        <div class="stat-value">${onlineSvcs}<span style="font-size:13px;color:var(--text-3)"> / ${totalSvcs} 在线</span></div>
        <div class="stat-sub">${totalSvcs===0 ? '还没有配置服务' : (onlineSvcs===totalSvcs ? '全部在线' : onlineSvcs===0 ? '未在线' : `${totalSvcs-onlineSvcs} 个离线`)}</div>
      </div>
      <div class="card">
        <div class="stat-label">插件</div>
        <div class="stat-value">${totalPlugins}<span style="font-size:13px;color:var(--text-3)"> 个</span></div>
        <div class="stat-sub">${totalPlugins===0 ? '插件目录为空' : `${onPlugins} 个标记为启用`}</div>
      </div>
    </div>
    <div class="card ov-logs">
      <div class="ov-logs-head">
        <b>最近日志</b>
        <div style="display:flex;align-items:center;gap:14px">
          <span class="ov-bot"><span class="dot ${state.botRunning?'ok':'off'}"></span>机器人${state.botRunning?'运行中':'未运行'}</span>
          <a style="color:#e8a33d;cursor:pointer;font-size:12.5px" data-action="bot-kill" title="强制结束机器人进程：清除残留的 python bot.py，释放被占用的 8080 端口">强制结束</a>
          <a style="color:var(--accent);cursor:pointer;font-size:12.5px" data-action="go-logs">查看全部 →</a>
        </div>
      </div>
      <div class="log-body" id="mini-log"></div>
    </div>`;
  },

  /* 账号管理视图：遍历 state.accounts 渲染卡片，备注可点编辑图标改名（见 editNick）。
   * 为什么必须转义：a.nick 是用户自己填的 QQ 备注（存储型 XSS 入口），
   *   a.qq 来自后端账号列表；不转义用户写 `<img src=x onerror=...>` 就会在桌面端执行。
   *   a.color 是前端从 AVATAR_GRADIENTS 里挑的常量、i 是数组下标，都不可能带外部数据，
   *   这里仍然过一遍 escapeHtml 保持"插值一律转义"的约定（对调色板值是无副作用的空操作）。 */
  accounts(){
    const cards = state.accounts.map((a,i)=>{
      /* "这个号是否正在跑"是瞬时事实，必须由 runningQq 推导，不能读持久字段 a.enabled：
       * 一个处于禁用态的号也可能正跑着（禁用只影响下次启动的选择）。 */
      const isRunning = state.running && state.runningQq === a.qq;
      return `
      <div class="card acct-card">
        <div class="acct-head">
          <div class="acct-avatar" style="background:${escapeHtml(a.color)}">${escapeHtml(a.qq.slice(-2))}</div>
          <div class="acct-meta">
            <div class="acct-nick" id="nick-${i}">
              ${escapeHtml(a.nick)}
              <span class="edit-icon" data-action="acct-edit-nick" data-i="${i}" title="编辑备注">
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/></svg>
              </span>
            </div>
            <div class="acct-qq">${escapeHtml(a.qq)}</div>
            <div style="margin-top:4px">
              ${a.saved
                ? '<span class="auth-tag ok">已记住登录态</span>'
                : '<span class="auth-tag warn">需扫码登录</span>'}
            </div>
          </div>
          ${statusPill(isRunning)}
        </div>
        <div class="acct-foot">
          <span style="font-size:11.5px;color:var(--text-3);font-family:var(--mono)">config/napcat_${escapeHtml(a.qq)}.json</span>
          <div style="display:flex;align-items:center;gap:8px">
            <span style="font-size:11px;color:var(--text-3)">启用</span>
            <div class="switch ${a.enabled!==false?'on':''}" data-action="acct-enable" data-qq="${escapeHtml(a.qq)}" title="启用/禁用此账号（禁用后启动时不带该号）"></div>
            <button class="btn sm danger" data-action="acct-delete" data-qq="${escapeHtml(a.qq)}" title="删除此账号配置">删除</button>
            <button class="btn sm ${isRunning?'danger':'primary'}" data-action="acct-toggle" data-qq="${escapeHtml(a.qq)}">${isRunning?'停止':'启动'}</button>
          </div>
        </div>
      </div>`;
    }).join('');
    return `
    <h2 class="view-title">账号管理</h2>
    <div class="grid grid-3">
      ${cards}
      <div class="add-card" data-action="acct-add">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
        <div style="font-size:13px">添加 QQ 账号</div>
      </div>
    </div>
    <div class="empty-note">每个账号在桌面端对应一份配置；启动时由 NapCat 拉起对应 QQ 号。</div>`;
  },

  /* OneBot 网络视图：HTTP/WS/反向WS 服务列表 + 内联表单。
   * 表单字段：类型、端口、地址（反向WS填）、Token、备注。
   * 保存时整体写回 cfg.onebot，后端热重载。 */
  network(){
    const rows = state.services.map((s,i)=>{
      // 如果正在编辑这条，渲染成表单
      if(state.netForm && state.netForm.editing && state.netForm.i === i){
        return netFormCard(state.netForm);
      }
      return `
      <div class="net-row">
        <span class="net-type ${s.type==='HTTP'?'http':s.type==='WS'?'ws':'post-ws'}">${escapeHtml(s.type)}</span>
        <div class="net-info">
          <div class="net-port">:${escapeHtml(String(s.port))}</div>
          <div class="net-addr">${escapeHtml(s.addr||(s.type==='WS'?'本地服务':'-'))}${s.token?` <span style="color:var(--text-3);font-size:11px">· token:${escapeHtml(s.token.slice(0,4))}***</span>`:''}</div>
          <div class="net-memo-text">${escapeHtml(s.memo||'暂无备注')}</div>
        </div>
        ${statusPill(s.running)}
        <div class="switch ${s.enabled!==false?'on':''}" data-action="net-toggle" data-i="${i}" title="启用/禁用"></div>
        <button class="btn sm ghost" data-action="net-edit" data-i="${i}">编辑</button>
        <button class="btn sm ghost danger" data-action="net-delete" data-i="${i}">删除</button>
      </div>`;
    }).join('');
    // 新建模式时在列表顶部插入表单
    const newForm = (state.netForm && !state.netForm.editing) ? netFormCard(state.netForm) : '';
    return `
    <h2 class="view-title">OneBot 网络配置</h2>
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:14px">
      <div class="empty-note" style="margin:0">这些配置写入 config/napcat_&lt;QQ&gt;.json，保存后自动热重载。</div>
      <button class="btn primary sm" data-action="net-new">+ 新建服务</button>
    </div>
    ${newForm}
    ${rows || '<div class="empty-note" style="margin-top:30px;text-align:center">还没有 OneBot 服务，点右上角"+ 新建服务"添加。</div>'}`;
  },

  /* 插件视图：遍历 state.plugins 渲染卡片。
   * 为什么必须转义：name/version/desc/author/dir 全部来自磁盘上第三方插件的
   *   package.json——用户把插件文件夹丢进 plugins/ 目录即可加载，这是典型的
   *   存储型 XSS；在 Tauri WebView 里等于脚本可触发任意后端 command（RCE 面），
   *   一个字符都不能裸拼。 */
  plugins(){
    /* 已安装插件按"是否来自本面板的推荐清单"分成两块：
     *   默认插件 = 目录名能在 RECOMMENDED_PLUGINS 里找到（从推荐区装来的 / 官方出品）
     *   自定义插件 = 其余（你自己丢进插件目录的）
     * 判定用**目录名**而不是备注，因为备注是用户可以随便改的展示字段。
     * 为什么必须转义：name/desc/author 来自磁盘上第三方插件的 package.json，
     * 用户把文件夹丢进 plugins/ 就会被读出来——这是典型的存储型 XSS 入口，
     * 在 Tauri WebView 里等于脚本能调后端 command，一个字符都不能裸拼。 */
    const recIds = new Set(RECOMMENDED_PLUGINS.map(r=>r.id));
    const installedIds = new Set(state.plugins.map(p=>p.dir));
    const isDefault = p => recIds.has(p.dir);

    const installedCell = (p, i) => {
      const memo = (state.cfg.plugin_memos||{})[p.dir] || '';
      const title = memo || p.name;   // 大标题优先用备注
      const sub = memo ? p.name : ''; // 有备注时才把插件名当小字显示
      return `
      <div class="plugin-cell" title="${escapeHtml(p.desc||'')}">
        <div class="plugin-cell-icon" style="background:${escapeHtml(p.color||'linear-gradient(135deg,#5ab0ff,#a06bff)')}">${escapeHtml(p.name.slice(0,1).toUpperCase())}</div>
        <div class="plugin-cell-name" title="${escapeHtml(title)}">${escapeHtml(title)}</div>
        ${sub ? `<div class="plugin-cell-sub">${escapeHtml(sub)}</div>` : ''}
        <div class="plugin-cell-ver">v${escapeHtml(p.version||'0.0.0')}</div>
        <div class="plugin-cell-toggle" title="状态取自插件目录名是否带 .disabled 后缀">
          <span class="auth-tag ${p.on?'ok':'warn'}">${p.on?'已启用':'已禁用'}</span>
        </div>
        <div class="plugin-cell-memo" data-action="plugin-edit-memo" data-i="${i}" title="点击编辑备注">${memo?'编辑备注…':'添加备注…'}</div>
      </div>`;
    };
    /* 保留原始下标 i：plugin-edit-memo 靠 data-i 索引 state.plugins */
    const indexed = state.plugins.map((p,i)=>({p,i}));
    const defaultCells = indexed.filter(x=>isDefault(x.p)).map(x=>installedCell(x.p,x.i)).join('');
    const customCells  = indexed.filter(x=>!isDefault(x.p)).map(x=>installedCell(x.p,x.i)).join('');
    const defaultCount = indexed.filter(x=>isDefault(x.p)).length;
    const customCount  = indexed.length - defaultCount;

    /* 可选插件 = 推荐清单里**还没装**的那些 */
    const available = RECOMMENDED_PLUGINS.filter(r=>!installedIds.has(r.id));
    const recCell = (r) => `
      <div class="plugin-cell pl-rec" title="${escapeHtml(r.desc)}">
        <div class="plugin-cell-icon" style="background:${r.official?'linear-gradient(135deg,#ff9f43,#ff6b6b)':'linear-gradient(135deg,#5ab0ff,#a06bff)'}">${escapeHtml(r.name.slice(0,1).toUpperCase())}</div>
        <div class="plugin-cell-name" title="${escapeHtml(r.name)}">${escapeHtml(r.name)}</div>
        <div class="plugin-cell-sub">${escapeHtml(r.author)} · <span class="pl-rec-tag ${r.official?'official':'community'}">${r.official?'官方':'社区'}</span></div>
        <div class="plugin-cell-desc">${escapeHtml(r.desc)}</div>
        <div class="pl-rec-actions">
          <button class="btn sm ghost" data-action="plugin-open-repo" data-url="${escapeHtml(r.repo)}">仓库</button>
          ${r.zip
            ? `<button class="btn sm primary" data-action="plugin-install" data-id="${escapeHtml(r.id)}" data-name="${escapeHtml(r.id)}" data-url="${escapeHtml(r.zip)}">安装</button>`
            : `<button class="btn sm ghost" disabled title="该仓库没有提供可直接安装的包，需要按仓库说明自行构建">需自建</button>`}
        </div>
      </div>`;

    return `
    <h2 class="view-title">插件</h2>
    <div class="card" style="margin-bottom:13px;display:flex;align-items:center;gap:14px;flex-wrap:wrap;padding:12px 16px">
      <div style="flex:1;min-width:0">
        <div style="font-size:12.5px;color:var(--text-2)">
          插件目录：<span style="font-family:var(--mono);color:var(--text-1)">${escapeHtml((state.cfg&&state.cfg.plugins_dir)||'未配置')}</span>
        </div>
        <div class="empty-note" style="margin:2px 0 0">把插件文件夹放进此目录，点刷新即可识别（读取其中的 package.json）。</div>
        <!-- 诚实标注：本面板没有"启用/停用插件"的能力（后端无此命令），
             卡片上的状态只是按目录名后缀的只读展示，改状态请去 NapCat 自带 WebUI。 -->
        <div class="empty-note" style="margin:2px 0 0">插件的启用 / 停用请在 NapCat 自带的 WebUI 插件市场中操作；本面板仅按目录名是否带 <code>.disabled</code> 后缀展示状态。</div>
      </div>
      <button class="btn sm ghost" data-action="plugin-open-dir">打开目录</button>
      <button class="btn sm primary" data-action="plugin-refresh">刷新</button>
      <button class="btn sm ${state.botRunning?'danger':'primary'}" data-action="bot-toggle">${state.botRunning?'停止机器人':'启动机器人'}</button>
      <button class="btn sm ghost" data-action="bot-kill" title="强制结束机器人进程：清除残留的 python bot.py，释放被占用的 8080 端口">强制结束</button>
    </div>

    <div class="card pl-block" style="padding:16px 18px">
      <div class="pl-block-head">
        <b>默认插件</b><span class="pl-count">${defaultCount}</span>
        <span class="pl-hint">来自本面板推荐清单（按目录名匹配）</span>
      </div>
      <div class="plugin-grid">${defaultCells || '<div class="pl-empty">还没装任何推荐插件，可以在下面「可选插件」里点「安装」。</div>'}</div>
    </div>

    <div class="card pl-block" style="padding:16px 18px">
      <div class="pl-block-head">
        <b>自定义插件</b><span class="pl-count">${customCount}</span>
        <span class="pl-hint">你自己放进插件目录的插件</span>
      </div>
      <div class="plugin-grid">${customCells || '<div class="pl-empty">暂无自定义插件，把插件文件夹或 .zip 拖到页面底部虚线框即可安装。</div>'}</div>
    </div>

    <div class="card pl-block" style="padding:16px 18px">
      <div class="pl-block-head">
        <b>可选插件</b><span class="pl-count">${available.length}</span>
        <span class="pl-hint">精选自 NapCat 官方插件索引与 GitHub 社区，点「安装」自动下载到插件目录</span>
      </div>
      <div class="plugin-grid">${available.map(recCell).join('') || '<div class="pl-empty">推荐清单里的插件都已经装上了。</div>'}</div>
    </div>

    <div id="plugin-dropzone" style="margin-top:2px;padding:22px;text-align:center;border:2px dashed var(--border-strong);border-radius:10px;color:var(--text-3);font-size:13px;transition:all .15s">
      把插件文件夹或 .zip 文件拖到这里安装
    </div>`;
  },

  /* 实时日志视图：过滤栏 + 日志容器。日志行的实际渲染在 renderLogs()，这里只搭骨架。 */
  logs(){
    return `
    <h2 class="view-title">实时日志</h2>
    <div class="log-toolbar">
      <select id="log-level">
        <option value="">全部级别</option>
        <option value="recv">QQ消息</option>
        <option value="plugin">插件加载</option>
        <option value="info">INFO</option>
        <option value="ok">OK</option>
        <option value="warn">WARN</option>
        <option value="err">ERROR</option>
      </select>
      <select id="log-source" title="按来源筛选">
        <option value="">全部来源</option>
        <option value="napcat">NapCat</option>
        <option value="ai">AI</option>
        <option value="bot">机器人</option>
      </select>
      <input type="text" id="log-filter" placeholder="过滤关键字…" style="width:200px">
      <button class="btn sm ghost" id="btn-pause" data-action="log-pause">${pauseLabel()}</button>
      <button class="btn sm ghost" data-action="log-clear">清空</button>
      <button class="btn sm ghost" data-action="log-export">导出</button>
    </div>
    <div class="log-body" id="log-body" style="height:calc(100vh - 280px);min-height:300px"></div>`;
  },

  /* 内嵌 WebUI 视图：生产环境这里其实是 Tauri WebView 直接加载
   * http://127.0.0.1:6099/webui?token=...，当前用浏览器框样式做占位。 */
  webui(){
    return `
    <h2 class="view-title">内嵌 WebUI</h2>
    <div class="browser-mock">
      <div class="browser-bar">
        <div class="browser-dots">
          <span style="background:#ff5f57"></span><span style="background:#febc2e"></span><span style="background:#28c840"></span>
        </div>
        <div class="url-box"><svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="vertical-align:-1px;margin-right:5px"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>http://127.0.0.1:6099/webui?token=sk-9f2a****</div>
        <button class="btn sm ghost" data-action="webui-open-external">外部打开</button>
      </div>
      <div class="browser-content">
        <div style="text-align:center">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#aab" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="margin-bottom:10px"><path d="M12 5c.67 0 1.35.03 2 .1v2.5"/><path d="M4.5 9.5C3.8 8 3 6.5 3.5 5c.3-.9 1.2-1.2 2-.8C6 5 6.8 5.7 7.3 6.6"/><path d="M19.5 9.5c.7-1.5 1.5-3 1-4.5-.3-.9-1.2-1.2-2-.8C18 5 17.2 5.7 16.7 6.6"/><circle cx="12" cy="13" r="6.5"/></svg>
          <div>NapCat WebUI 已嵌入此窗口</div>
          <div style="font-size:12px;margin-top:6px;color:#aab">生产环境由 Tauri WebView 直接加载，无需跳转浏览器</div>
        </div>
      </div>
    </div>`;
  },

  /* 设置视图：NapCat/QQ 路径检测 + 开关/下拉/代理。所有控件改动最终走 save_config 持久化。 */
  settings(){
    return `
    <h2 class="view-title">设置</h2>
    <div class="card" style="margin-bottom:14px">
      <div class="set-row">
        <div>
          <div class="set-label">NapCat 工作目录</div>
          <div class="set-desc">launcher.bat 所在目录，所有 config/ 与 logs/ 都在这里</div>
          <div class="path-status" id="status-napcat"></div>
        </div>
        <div class="path-row">
          <input type="text" id="path-napcat" value="${state.cfg&&state.cfg.napcat_path?escapeHtml(state.cfg.napcat_path):'D:\\Apps\\NapCat.Shell'}" readonly>
          <button class="btn sm ghost" data-action="browse-napcat">浏览</button>
          <button class="btn sm primary" id="btn-detect-napcat" data-action="detect-napcat">自动检测</button>
        </div>
      </div>
      <div class="set-row">
        <div>
          <div class="set-label">QQ 安装路径</div>
          <div class="set-desc">用于自动检测 QQ 版本兼容性，会扫描注册表与常见安装目录</div>
          <div class="path-status" id="status-qq"></div>
        </div>
        <div class="path-row">
          <input type="text" id="path-qq" value="${state.cfg&&state.cfg.qq_path?escapeHtml(state.cfg.qq_path):'C:\\Program Files\\Tencent\\QQNT'}" readonly>
          <button class="btn sm ghost" data-action="browse-qq">浏览</button>
          <button class="btn sm primary" id="btn-detect-qq" data-action="detect-qq">自动检测</button>
        </div>
      </div>
    </div>
    <!-- 开关/下拉/代理：变更即写回 state.cfg 并 save_config 整体覆盖持久化（后端不支持增量 patch）；
         开关态在 render 时从 load_config 结果回填 -->
    <div class="card">
      <div class="set-row">
        <div><div class="set-label">开机自启</div><div class="set-desc">登录 Windows 后自动启动此桌面端并恢复上次状态（后端本期仅持久化，不负责注册自启）</div></div>
        <div class="switch ${state.cfg&&state.cfg.start_on_boot?'on':''}" data-action="toggle-setting" data-key="start_on_boot"></div>
      </div>
      <div class="set-row">
        <div><div class="set-label">关闭时最小化到托盘</div><div class="set-desc">点 X 不退出，保留 NapCat 后台运行</div></div>
        <div class="switch ${state.cfg&&state.cfg.minimize_to_tray?'on':''}" data-action="toggle-setting" data-key="minimize_to_tray"></div>
      </div>
      <div class="set-row">
        <div><div class="set-label">崩溃自动重启</div><div class="set-desc">NapCat 子进程异常退出时自动拉起，复用本地登录态无需重新扫码</div></div>
        <div class="switch ${state.cfg&&state.cfg.auto_restart?'on':''}" data-action="toggle-setting" data-key="auto_restart"></div>
      </div>
      <div class="set-row">
        <div><div class="set-label">掉线检测间隔</div><div class="set-desc">每隔多久探测一次 QQ 在线心跳（后端配置暂未提供该字段，暂为静态展示）</div></div>
        <select><option>3 秒</option><option selected>5 秒</option><option>10 秒</option><option>30 秒</option></select>
      </div>
      <div class="set-row">
        <div><div class="set-label">最大自动重启次数</div><div class="set-desc">连续崩溃超过此次数后停止重启，避免死循环</div></div>
        <select data-action="setting-select" data-key="max_restarts">
          <option value="3" ${state.cfg&&state.cfg.max_restarts===3?'selected':''}>3 次</option>
          <option value="5" ${!state.cfg||state.cfg.max_restarts===5?'selected':''}>5 次</option>
          <option value="10" ${state.cfg&&state.cfg.max_restarts===10?'selected':''}>10 次</option>
        </select>
      </div>
      <div class="set-row">
        <div><div class="set-label">登录态提示</div><div class="set-desc">登录态由 NapCat 自身保存在它的工作目录；桌面端只记录该账号是否成功登录过，不保存也不加密任何 token</div></div>
        <span style="font-family:var(--mono);font-size:11.5px;color:var(--ok)">${state.accounts.filter(a=>a.saved).length} / ${state.accounts.length} 账号已登录过</span>
      </div>
      <div class="set-row">
        <div><div class="set-label">代理</div><div class="set-desc">HTTP / SOCKS5，留空则直连</div></div>
        <div class="path-row"><input type="text" placeholder="socks5://127.0.0.1:1080" value="${state.cfg?escapeHtml(state.cfg.proxy||''):''}" data-action="setting-proxy"></div>
      </div>
    </div>`;
  }
};
