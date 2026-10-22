/* 启动引导：把「加载配置 / 订阅事件 / 绑定交互」拆成独立步骤，boot() 只负责编排。
 * 由 dist/index.html 拆分；本文件依赖前面的脚本完成全局定义。
 */

/* ================= 登录结果判定 =================
 * 从 NapCat 的日志行里识别登录结果。抽成常量而不是写在回调里，
 * 一是便于对照 NapCat 各版本的文案变化，二是可以用控制台单独验证正则。
 */
const LOGIN_OK_RE = /登录成功|login\s*success|online|已上线|二维码验证|scan\s*success|账[号户].*上线/i;
const LOGIN_FAIL_RE = /登录失败|login\s*fail|登录超时|qrcode.*expired|二维码.*过期|二维码.*失效/i;
const BOT_EXIT_RE = /机器人进程已退出/;

/** 关闭二维码弹窗（幂等，弹窗没开也不报错）。 */
function closeQrcodeOverlay() {
  const ov = document.getElementById('qrcode-overlay');
  if (ov) ov.classList.remove('show');
}

/** 处理一行日志：入缓冲 → 必要时局部重绘 → 登录/机器人状态提示。 */
function handleLogPayload(payload) {
  const line = (payload && payload.line || '').toString();
  pushLog(payload && payload.level, line, payload && payload.source);

  /* 暂停时只累积、不重绘：日志已进 state.logs（pushLog 照收），
   * 恢复后能一次性看到暂停期间的日志，也避免自动滚动打断用户翻看。 */
  if (!state.pausedLogs) {
    if (location.hash.includes('#/logs')) renderLogs();
    if (location.hash.includes('#/overview')) renderOverviewLogs();
  }

  handleLoginHints(line);
}

/** 根据日志文案更新登录态与机器人态。
 *  为什么不在 applyStatus 里做：后端 spawn 成功就 emit running=true，
 *  那一刻用户可能还没扫码，用"进程起来了"当依据会让"已记住登录态"提前点亮。 */
function handleLoginHints(line) {
  if (LOGIN_OK_RE.test(line)) {
    closeQrcodeOverlay();
    toast('扫码登录成功', 'ok');
    /* 只有明确报出登录成功才写盘；只碰 saved、不碰 enabled（运行态/启用态分离）。
     * fire-and-forget：事件回调路径不能等 IPC 往返。 */
    if (state.runningQq) {
      const cur = state.accounts.find(a => a.qq === state.runningQq);
      if (cur && !cur.saved) {
        cur.saved = true;
        saveConfig().catch(() => {});
        render();
      }
    }
    return;
  }

  if (LOGIN_FAIL_RE.test(line)) {
    closeQrcodeOverlay();
    toast('扫码失败或二维码已过期，请重新启动', 'warn');
    return;
  }

  if (BOT_EXIT_RE.test(line)) {
    toast('机器人进程已退出，可能崩溃或报错', 'warn');
    state.botRunning = false;
    render();
  }
}

/** 读配置；napcat_path 为空时自动探测一次并落盘，然后让后端重新推断 plugins_dir。 */
async function loadInitialConfig() {
  const cfg = await callCmd('load_config');
  if (!cfg) return;
  hydrateConfig(cfg);

  if (!cfg.napcat_path) {
    const cands = await callCmd('detect_napcat_path');
    if (Array.isArray(cands) && cands.length) {
      cfg.napcat_path = cands[0];
      cfg.plugins_dir = '';
      await callCmd('save_config', { config: cfg });
      const fresh = await callCmd('load_config');
      if (fresh) hydrateConfig(fresh);
    }
  }

  await reloadPlugins();
}

/** 订阅后端事件。先注册再拉状态，避免漏掉启动瞬间的推送。 */
async function subscribeEvents() {
  const { listen } = window.__TAURI__.event;

  await listen('napcat://log', e => handleLogPayload(e.payload));
  await listen('napcat://status', e => applyStatus(e.payload));
  await listen('napcat://qrcode', e => {
    const img = document.getElementById('qrcode-img');
    if (img && e.payload && e.payload.data_url) {
      img.src = e.payload.data_url;
      const ov = document.getElementById('qrcode-overlay');
      if (ov) ov.classList.add('show');
    }
  });

  const closeBtn = document.getElementById('qrcode-close');
  if (closeBtn) closeBtn.addEventListener('click', closeQrcodeOverlay);
}

/** 拖放安装插件：只在插件页响应，把拖入的文件夹/zip 交给 install_plugin。 */
async function setupDragDrop() {
  const wv = window.__TAURI__.webview.getCurrentWebview();
  await wv.onDragDropEvent(async (event) => {
    if (!location.hash.includes('#/plugins')) return;
    const dz = document.getElementById('plugin-dropzone');
    if (!dz) return;

    const p = event.payload;
    if (p.type === 'enter' || p.type === 'over') {
      dz.style.borderColor = 'var(--accent)';
      dz.style.background = 'color-mix(in srgb, var(--accent) 10%, transparent)';
    } else if (p.type === 'leave') {
      dz.style.borderColor = 'var(--border)';
      dz.style.background = 'transparent';
    } else if (p.type === 'drop') {
      dz.style.borderColor = 'var(--border)';
      dz.style.background = 'transparent';
      for (const path of (p.paths || [])) {
        try {
          const r = await callCmd('install_plugin', { source: path });
          toast(r || '已安装');
        } catch (err) {
          toast('安装失败: ' + (err && err.message || err), 'warn');
        }
      }
      await reloadPlugins();
      render();
    }
  });
}

/** 首次启动引导。顺序：空态 → 配置 → 事件 → 拖放 → 运行时状态。 */
async function boot() {
  render();                              // 先给空态，配置失败也不至于白屏
  if (!window.__TAURI__) return;          // 浏览器里调试到此为止

  try {
    await loadInitialConfig();
    await subscribeEvents();

    try {
      await setupDragDrop();
    } catch (err) {
      console.warn('拖放监听注册失败:', err);
    }

    const st = await callCmd('get_napcat_status');
    if (st) applyStatus(st); else render();

    try {
      state.botRunning = await callCmd('get_bot_status') || false;
    } catch (_) { /* 机器人状态拿不到不影响主界面 */ }

    render();
  } catch (err) {
    console.error('boot 初始化失败:', err);
    toast('初始化部分失败: ' + (err && err.message || err), 'warn');
    render();
  }
}

boot();
