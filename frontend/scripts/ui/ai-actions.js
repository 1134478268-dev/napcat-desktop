/* AI 页交互：数据加载/保存、服务与人设增删改、记忆管理、对话。
 * 注册 views.ai 与 PAGE_NAMES.ai —— 必须在 router.js 之后加载（见 index.html 顺序）。
 */

views.ai = aiView;
PAGE_NAMES.ai = 'AI 接入';

const AI_DEFAULT_CONFIG = {
  enabled: true,
  active_provider: '',
  providers: [],
  active_persona: '',
  system_extra: '',
  temperature: 0.7,
  max_tokens: 2048,
  memory_enabled: true,
  memory_limit: 50,
  tool_calling_enabled: false,
};

function aiCfg() {
  if (!state.ai.config) state.ai.config = Object.assign({}, AI_DEFAULT_CONFIG);
  return state.ai.config;
}

function aiNewId(prefix) {
  return prefix + '-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
}

function aiNow() {
  const d = new Date();
  const p = n => String(n).padStart(2, '0');
  return d.getFullYear() + '-' + p(d.getMonth() + 1) + '-' + p(d.getDate()) +
    ' ' + p(d.getHours()) + ':' + p(d.getMinutes());
}

async function aiLoadAll() {
  const st = await callCmd('ai_load_state');
  if (!st) {
    /* 加载失败时绝对不能拿默认值继续跑并落盘：
     * 默认 AiConfig 的 providers 是空数组，一旦被 ai_save_config 写回，
     * 用户配好的服务 / 人设 / 记忆就永久丢了（这个坑真实发生过）。 */
    state.ai.loaded = true;
    state.ai.loadFailed = true;
    render();
    return;
  }
  state.ai.loadFailed = false;
  state.ai.config = Object.assign({}, AI_DEFAULT_CONFIG, st.config || {});
  state.ai.personas = st.personas || [];
  state.ai.memories = st.memories || [];
  state.ai.gatewayStatus = (await callCmd('ai_gateway_status')) || {};
  state.ai.loaded = true;
  render();
}

/* 注意：callCmd 对 `Result<(), String>` 这类命令，成功时返回的是 null，
 * 所以不能用 `!== null` 判断成功（那会恒为假，导致保存后流程静默中断）。
 * 这里直接用 invoke + try/catch，成功与否看是否抛异常。 */
async function aiInvokeStrict(cmd, args) {
  if (!window.__TAURI__) {
    toast('未在 Tauri 环境运行', 'warn');
    return false;
  }
  try {
    await window.__TAURI__.core.invoke(cmd, args);
    return true;
  } catch (e) {
    toast('保存失败：' + String(e), 'warn');
    return false;
  }
}

async function aiPersistConfig() {
  if (state.ai.loadFailed) {
    toast('配置未成功加载，已阻止写入以防覆盖磁盘数据', 'warn');
    return false;
  }
  return aiInvokeStrict('ai_save_config', { config: aiCfg() });
}

async function aiPersistPersonas() {
  if (state.ai.loadFailed) return false;
  return aiInvokeStrict('ai_save_personas', { personas: state.ai.personas });
}

async function aiPersistMemories() {
  if (state.ai.loadFailed) return false;
  return aiInvokeStrict('ai_save_memories', { memories: state.ai.memories });
}

function aiVal(id) {
  const el = document.getElementById(id);
  return el ? el.value.trim() : '';
}

/* ---------------- 服务 ---------------- */

function aiAddProvider() {
  state.ai.providerForm = { preset: '', name: '', kind: 'openai', api_base: '', model: '', api_key: '' };
  render();
}

function aiEditProvider(id) {
  const p = (aiCfg().providers || []).find(x => x.id === id);
  if (!p) return;
  state.ai.providerForm = Object.assign({ editing: id }, p);
  render();
}

async function aiSaveProvider() {
  const f = state.ai.providerForm;
  if (!f) return;
  const name = aiVal('ai-p-name');
  const base = aiVal('ai-p-base');
  const model = aiVal('ai-p-model');
  const key = aiVal('ai-p-key');
  const kind = aiVal('ai-p-kind') || 'openai';
  if (!name) { toast('请填写名称', 'warn'); return; }
  if (!base) { toast('请填写 API 地址', 'warn'); return; }

  const cfg = aiCfg();
  if (!cfg.providers) cfg.providers = [];

  if (f.editing) {
    const p = cfg.providers.find(x => x.id === f.editing);
    if (!p) return;
    Object.assign(p, { name, api_base: base, model, api_key: key, kind });
  } else {
    const p = { id: aiNewId('p'), name, kind, api_base: base, model, api_key: key, enabled: true };
    cfg.providers.push(p);
    if (!cfg.active_provider) cfg.active_provider = p.id;
  }

  state.ai.providerForm = null;
  if (await aiPersistConfig()) toast('已保存 AI 服务', 'ok');
  render();
}

async function aiDeleteProvider(id) {
  const cfg = aiCfg();
  cfg.providers = (cfg.providers || []).filter(p => p.id !== id);
  if (cfg.active_provider === id) cfg.active_provider = (cfg.providers[0] || {}).id || '';
  if (await aiPersistConfig()) toast('已删除', 'ok');
  render();
}

async function aiSetProvider(id) {
  aiCfg().active_provider = id;
  if (await aiPersistConfig()) render();
}

function aiApplyPreset(sel) {
  const p = AI_PRESETS.find(x => x.id === sel.value);
  if (!p) return;
  const set = (id, v) => { const el = document.getElementById(id); if (el) el.value = v; };
  set('ai-p-name', p.name);
  set('ai-p-kind', p.kind);
  set('ai-p-base', p.api_base);
  set('ai-p-model', p.model);
}

/* ---------------- 人设 ---------------- */

function aiAddPersona() {
  state.ai.personaForm = { name: '', prompt: '' };
  render();
}

function aiEditPersona(id) {
  const p = state.ai.personas.find(x => x.id === id);
  if (!p) return;
  state.ai.personaForm = Object.assign({ editing: id }, p);
  render();
}

async function aiSavePersona() {
  const f = state.ai.personaForm;
  if (!f) return;
  const name = aiVal('ai-persona-name');
  const prompt = aiVal('ai-persona-prompt');
  if (!name) { toast('请填写人设名称', 'warn'); return; }

  if (f.editing) {
    const p = state.ai.personas.find(x => x.id === f.editing);
    if (p) Object.assign(p, { name, prompt });
  } else {
    const p = { id: aiNewId('persona'), name, prompt, created_at: aiNow() };
    state.ai.personas.push(p);
    if (!aiCfg().active_persona) aiCfg().active_persona = p.id;
  }

  state.ai.personaForm = null;
  await aiPersistPersonas();
  await aiPersistConfig();
  toast('人设已保存', 'ok');
  render();
}

async function aiDeletePersona(id) {
  state.ai.personas = state.ai.personas.filter(p => p.id !== id);
  const cfg = aiCfg();
  if (cfg.active_persona === id) cfg.active_persona = (state.ai.personas[0] || {}).id || '';
  await aiPersistPersonas();
  await aiPersistConfig();
  render();
}

async function aiSetPersona(id) {
  aiCfg().active_persona = id;
  if (await aiPersistConfig()) render();
}

/* ---------------- 记忆 ---------------- */

async function aiAddMemory() {
  const text = aiVal('ai-memory-input');
  if (!text) { toast('请输入记忆内容', 'warn'); return; }
  state.ai.memories.push({
    id: aiNewId('mem'),
    persona_id: aiCfg().active_persona || '',
    content: text,
    created_at: aiNow(),
    source: 'manual',
  });
  await aiPersistMemories();
  render();
}

async function aiDeleteMemory(id) {
  state.ai.memories = state.ai.memories.filter(m => m.id !== id);
  await aiPersistMemories();
  render();
}

async function aiClearMemories() {
  if (!state.ai.memories.length) return;
  if (!confirm('清空全部记忆？该操作立即写入磁盘。')) return;
  state.ai.memories = [];
  await aiPersistMemories();
  toast('记忆已清空', 'ok');
  render();
}

async function aiToggleMemory(el) {
  aiCfg().memory_enabled = !!el.checked;
  await aiPersistConfig();
}

async function aiToggleTools(el) {
  aiCfg().tool_calling_enabled = !!el.checked;
  await aiPersistConfig();
  toast(el.checked ? '已打开工具能力（当前仅写入提示词，不会真正执行）' : '已关闭工具能力', 'warn');
}

/* ---------------- 对话 ---------------- */

function aiClearChat() {
  state.ai.chat = [];
  render();
}

async function aiSend() {
  if (state.ai.busy) return;
  const input = document.getElementById('ai-chat-input');
  const text = input ? input.value.trim() : '';
  if (!text) return;

  const cfg = aiCfg();
  const provider = (cfg.providers || []).find(p => p.id === cfg.active_provider) || (cfg.providers || [])[0];
  if (!provider) { toast('请先添加并选择一个 AI 服务', 'warn'); return; }

  state.ai.chat.push({ role: 'user', content: text });
  state.ai.busy = true;
  render();

  /* 只带最近 20 条，避免上下文无限增长把 token 打爆 */
  const history = state.ai.chat
    .filter(m => !m.error)
    .slice(-20)
    .map(m => ({ role: m.role, content: m.content }));

  const res = await callCmd('ai_chat', { providerId: cfg.active_provider || provider.id, messages: history });

  state.ai.busy = false;
  if (res && res.content) {
    state.ai.chat.push({ role: 'assistant', content: res.content });
  } else {
    state.ai.chat.push({ role: 'assistant', content: '请求失败，请在上方 toast 或日志里查看原因。', error: true });
  }
  render();
}

/* ---------------- 动作注册 ---------------- */

Object.assign(actions, {
  'ai-reload': () => { state.ai.loaded = false; state.ai.loadFailed = false; aiLoadAll(); },
  'ai-add-provider': () => aiAddProvider(),
  'ai-edit-provider': (el) => aiEditProvider(el.dataset.id),
  'ai-del-provider': (el) => aiDeleteProvider(el.dataset.id),
  'ai-set-provider': (el) => aiSetProvider(el.dataset.id),
  'ai-save-provider': () => aiSaveProvider(),
  'ai-add-persona': () => aiAddPersona(),
  'ai-edit-persona': (el) => aiEditPersona(el.dataset.id),
  'ai-del-persona': (el) => aiDeletePersona(el.dataset.id),
  'ai-set-persona': (el) => aiSetPersona(el.dataset.id),
  'ai-save-persona': () => aiSavePersona(),
  'ai-cancel-form': () => { state.ai.providerForm = null; state.ai.personaForm = null; render(); },
  'ai-add-memory': () => aiAddMemory(),
  'ai-del-memory': (el) => aiDeleteMemory(el.dataset.id),
  'ai-clear-memory': () => aiClearMemories(),
  'ai-clear-chat': () => aiClearChat(),
  'ai-send': () => aiSend(),
});

CHANGE_ACTIONS.add('ai-apply-preset');
CHANGE_ACTIONS.add('ai-toggle-memory');
CHANGE_ACTIONS.add('ai-toggle-tools');

Object.assign(actions, {
  'ai-apply-preset': (el) => aiApplyPreset(el),
  'ai-toggle-memory': (el) => aiToggleMemory(el),
  'ai-toggle-tools': (el) => aiToggleTools(el),
});

/* 导航到 AI 页时按需加载一次（数据不常变，loaded 后不再重复请求） */
(function aiHookRouter() {
  if (state.ai.loaded) return;
  const check = () => {
    if (location.hash.includes('#/ai') && !state.ai.loaded) {
      aiLoadAll();
    }
  };
  window.addEventListener('hashchange', check);
  // 启动时若已是 ai 路由（刷新恢复），也要触发一次
  setTimeout(check, 0);
})();

/* 提供 Ctrl+Enter 发送 */
document.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    const t = e.target;
    if (t && t.id === 'ai-chat-input') {
      e.preventDefault();
      aiSend();
    }
  }
});

/* ---------------- 自动回复网关 ---------------- */

function aiGwReadForm() {
  const cb = id => {
    const el = document.getElementById(id);
    return !!(el && el.checked);
  };
  const num = (id, dft) => {
    const v = Number(aiVal(id));
    return Number.isFinite(v) && v >= 0 ? v : dft;
  };
  return {
    enabled: cb('ai-gw-enabled'),
    auto_start: cb('ai-gw-autostart'),
    reply_private: cb('ai-gw-private'),
    reply_group_at: cb('ai-gw-groupat'),
    save_stickers: cb('ai-gw-savesticker'),
    send_stickers: cb('ai-gw-sendsticker'),
    ws_url: aiVal('ai-gw-ws') || 'ws://127.0.0.1:3001',
    http_url: aiVal('ai-gw-http') || 'http://127.0.0.1:3000',
    access_token: aiVal('ai-gw-token'),
    context_rounds: Math.max(1, num('ai-gw-rounds', 10)),
    cooldown_ms: num('ai-gw-cooldown', 1500),
    max_reply_chars: Math.max(50, num('ai-gw-maxchars', 800)),
  };
}

async function aiGwSave(opts) {
  const silent = opts && opts.silent;
  aiCfg().gateway = aiGwReadForm();
  const ok = await aiPersistConfig();
  if (!ok) return false;
  if (!silent) toast('自动回复配置已保存', 'ok');
  return true;
}

async function aiGwToggle() {
  const running = !!(state.ai.gatewayStatus && state.ai.gatewayStatus.running);

  if (running) {
    await callCmd('ai_gateway_stop');
  } else {
    /* 先落盘再启动：否则用户改了配置点启动，跑的还是上一次的配置 */
    if (!(await aiGwSave({ silent: true }))) return;
    await callCmd('ai_gateway_start');
  }

  state.ai.gatewayStatus = (await callCmd('ai_gateway_status')) || {};
  render();
}

Object.assign(actions, {
  'ai-gw-save': () => aiGwSave(),
  'ai-gw-toggle': () => aiGwToggle(),
});

/* ---------------- AI 总开关 ---------------- */

/** 关闭总开关时顺带把网关停掉，避免「AI 已关闭但网关还在回消息」的矛盾状态。 */
async function aiToggleEnabled(el) {
  const on = !!(el && el.checked);
  aiCfg().enabled = on;

  if (!(await aiPersistConfig())) return;

  if (!on) {
    await callCmd('ai_gateway_stop');
    state.ai.gatewayStatus = (await callCmd('ai_gateway_status')) || {};
  }

  toast(on ? 'AI 接入已启用' : 'AI 接入已关闭（网关已停止）', on ? 'ok' : 'warn');
  render();
}

CHANGE_ACTIONS.add('ai-toggle-enabled');
Object.assign(actions, {
  'ai-toggle-enabled': (el) => aiToggleEnabled(el),
});
