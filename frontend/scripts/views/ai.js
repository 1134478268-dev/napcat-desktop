/* AI 接入页：纯渲染 + 轻量 markdown 渲染（支持 ``` 代码块）。
 * 本文件只定义函数，视图注册在 ui/ai-actions.js（它加载在 router.js 之后）。
 */

/** 轻量 markdown → HTML。只支持代码块、行内代码、粗体、换行。
 *
 *  为什么不引 markdown 库：前端零构建、无 npm 依赖，为这一个页面引入完整解析器
 *  得不偿失。这里先整体 escapeHtml 再按固定顺序处理，保证不会出现 XSS。
 *  代码块先替换成占位符，避免块内的换行被下面统一转成 <br> 破坏格式。 */
function aiRenderMarkdown(text) {
  if (!text) return '';
  const blocks = [];
  let html = escapeHtml(String(text));

  html = html.replace(/```([\w+-]*)\n([\s\S]*?)```/g, (_m, lang, code) => {
    blocks.push(
      '<pre class="ai-code"><code data-lang="' + escapeHtml(lang || '') + '">' +
      code.replace(/\n$/, '') + '</code></pre>'
    );
    return '\u0000B' + (blocks.length - 1) + '\u0000';
  });

  html = html.replace(/`([^`\n]+)`/g, '<code class="ai-inline">$1</code>');
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/\n/g, '<br>');
  html = html.replace(/\u0000B(\d+)\u0000/g, (_m, i) => blocks[+i]);
  return html;
}

/** 密钥打码：保留前 4 位便于辨认是哪把 key，其余用 * 覆盖。 */
function aiMaskKey(key) {
  const k = String(key || '');
  if (!k) return '未填写';
  if (k.length <= 8) return '********';
  return k.slice(0, 4) + '••••••••' + k.slice(-2);
}

const AI_PRESETS = [
  { id: 'deepseek', name: 'DeepSeek', kind: 'openai', api_base: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { id: 'openai', name: 'OpenAI', kind: 'openai', api_base: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { id: 'moonshot', name: 'Moonshot / Kimi', kind: 'openai', api_base: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  { id: 'dashscope', name: '通义千问', kind: 'openai', api_base: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-plus' },
  { id: 'zhipu', name: '智谱 GLM', kind: 'openai', api_base: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4-flash' },
  { id: 'ollama', name: 'Ollama（本地）', kind: 'openai', api_base: 'http://127.0.0.1:11434/v1', model: 'llama3.1' },
  { id: 'anthropic', name: 'Anthropic Claude', kind: 'anthropic', api_base: 'https://api.anthropic.com/v1', model: 'claude-sonnet-4-5' },
  { id: 'gemini', name: 'Google Gemini', kind: 'gemini', api_base: 'https://generativelanguage.googleapis.com/v1beta', model: 'gemini-2.0-flash' },
];

/** AI 页主模板。 */
function aiView() {
  const ai = state.ai;
  if (ai.loadFailed) {
    return '<h2 class="view-title">AI 接入</h2>' +
      '<div class="card"><div class="empty-hint">读取 AI 配置失败。为避免覆盖磁盘上的配置，编辑已暂停。<br><br>' +
      '<button class="btn primary sm" data-action="ai-reload">重试</button></div></div>';
  }
  if (!ai.loaded) {
    return '<h2 class="view-title">AI 接入</h2><div class="card"><div class="empty-hint">正在读取 AI 配置…</div></div>';
  }

  const cfg = ai.config || {};
  const providers = cfg.providers || [];
  const active = providers.find(p => p.id === cfg.active_provider) || providers[0] || null;
  const persona = (ai.personas || []).find(p => p.id === cfg.active_persona) || null;

  const statusPillHtml = active
    ? '<span class="status-pill" style="background:var(--ok-soft);color:var(--ok)"><span class="dot ok"></span>已配置 ' + escapeHtml(active.name) + '</span>'
    : '<span class="status-pill" style="background:var(--warn-soft);color:var(--warn)"><span class="dot warn"></span>未配置</span>';

  const enabled = cfg.enabled !== false;

  return `
  <h2 class="view-title">AI 接入</h2>

  ${aiMasterSwitch(enabled)}

  ${enabled ? `
  <div class="card ai-hero">
    <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap">
      ${statusPillHtml}
      <span style="color:var(--text-3);font-size:12.5px">当前人设：${persona ? escapeHtml(persona.name) : '未选择'}</span>
      <span style="color:var(--text-3);font-size:12.5px">可用服务：${providers.length}</span>
      <span style="color:var(--text-3);font-size:12.5px">记忆：${(ai.memories || []).length} 条</span>
    </div>
  </div>

  ${aiProviderSection(cfg, providers, active)}
  ${aiPersonaSection(cfg, persona)}
  ${aiMemorySection(cfg, ai.memories || [])}
  ${aiGatewaySection(cfg)}
  ${aiChatSection(ai)}
  ` : '<div class="card"><div class="empty-hint">AI 接入已关闭。配置都还在，打开上方总开关即可恢复。</div></div>'}
  `;
}

/** 总开关：关闭后 AI 页只显示它自己，网关会被强制停止。 */
function aiMasterSwitch(enabled) {
  return `
  <div class="card ai-master">
    <div style="display:flex;align-items:center;justify-content:space-between;gap:16px;flex-wrap:wrap">
      <div>
        <div class="card-title">AI 接入总开关</div>
        <div class="ai-hint">关闭后：对话、自动回复网关、所有 AI 调用立即停止；配置与人设/记忆仍然保留。</div>
      </div>
      <label class="ai-switch" style="font-size:13.5px;gap:8px">
        <input type="checkbox" id="ai-master-enabled" ${enabled ? 'checked' : ''} data-change="ai-toggle-enabled">
        <span style="color:${enabled ? 'var(--ok)' : 'var(--text-3)'}">${enabled ? '已启用' : '已关闭'}</span>
      </label>
    </div>
  </div>`;
}

function aiProviderSection(cfg, providers, active) {
  const row = p => `
    <div class="ai-row">
      <div class="ai-row-main">
        <div class="ai-row-title">${escapeHtml(p.name || p.id)}
          <span class="ai-tag">${escapeHtml(p.kind || 'openai')}</span>
          ${p.id === cfg.active_provider ? '<span class="ai-tag ai-tag-on">当前</span>' : ''}
        </div>
        <div class="ai-row-sub">${escapeHtml(p.model || '未填模型')} · ${escapeHtml(p.api_base || '未填地址')}</div>
        <div class="ai-row-sub">Key：${escapeHtml(aiMaskKey(p.api_key))}</div>
      </div>
      <div class="ai-row-actions">
        ${p.id === cfg.active_provider ? '' : `<button class="btn ghost sm" data-action="ai-set-provider" data-id="${escapeHtml(p.id)}">设为当前</button>`}
        <button class="btn ghost sm" data-action="ai-edit-provider" data-id="${escapeHtml(p.id)}">编辑</button>
        <button class="btn ghost sm" data-action="ai-del-provider" data-id="${escapeHtml(p.id)}">删除</button>
      </div>
    </div>`;

  return `
  <div class="card">
    <div class="card-head">
      <div class="card-title">AI 服务</div>
      <div style="display:flex;gap:8px">
        <button class="btn ghost sm" data-action="ai-add-provider">添加服务</button>
      </div>
    </div>
    ${providers.length ? providers.map(row).join('') : '<div class="empty-hint">还没有配置任何 AI 服务。点「添加服务」从预设里选一个。</div>'}
    ${state.ai.providerForm ? aiProviderForm(state.ai.providerForm) : ''}
  </div>`;
}

function aiProviderForm(form) {
  const presets = AI_PRESETS.map(p =>
    `<option value="${p.id}" ${form.preset === p.id ? 'selected' : ''}>${escapeHtml(p.name)}</option>`
  ).join('');
  return `
  <div class="ai-form">
    <div class="ai-form-row">
      <label>预设</label>
      <select id="ai-p-preset" data-change="ai-apply-preset">
        <option value="">（自定义）</option>
        ${presets}
      </select>
    </div>
    <div class="ai-form-row"><label>名称</label><input id="ai-p-name" value="${escapeHtml(form.name || '')}" placeholder="例如 DeepSeek"></div>
    <div class="ai-form-row"><label>协议</label>
      <select id="ai-p-kind">
        <option value="openai" ${form.kind === 'openai' ? 'selected' : ''}>OpenAI 兼容</option>
        <option value="anthropic" ${form.kind === 'anthropic' ? 'selected' : ''}>Anthropic</option>
        <option value="gemini" ${form.kind === 'gemini' ? 'selected' : ''}>Gemini</option>
      </select>
    </div>
    <div class="ai-form-row"><label>API 地址</label><input id="ai-p-base" value="${escapeHtml(form.api_base || '')}" placeholder="https://api.deepseek.com/v1"></div>
    <div class="ai-form-row"><label>模型</label><input id="ai-p-model" value="${escapeHtml(form.model || '')}" placeholder="deepseek-chat"></div>
    <div class="ai-form-row"><label>API Key</label><input id="ai-p-key" type="password" value="${escapeHtml(form.api_key || '')}" placeholder="sk-..."></div>
    <div class="ai-form-actions">
      <button class="btn primary sm" data-action="ai-save-provider">保存</button>
      <button class="btn ghost sm" data-action="ai-cancel-form">取消</button>
      <span class="ai-hint">Key 只保存在本机 %APPDATA%\\com.napcat.desktop\\ai\\config.json</span>
    </div>
  </div>`;
}

function aiPersonaSection(cfg, active) {
  const rows = (state.ai.personas || []).map(p => `
    <div class="ai-row">
      <div class="ai-row-main">
        <div class="ai-row-title">${escapeHtml(p.name)}
          ${p.id === cfg.active_persona ? '<span class="ai-tag ai-tag-on">当前</span>' : ''}
        </div>
        <div class="ai-row-sub ai-prompt-preview">${escapeHtml((p.prompt || '').slice(0, 120))}${(p.prompt || '').length > 120 ? '…' : ''}</div>
      </div>
      <div class="ai-row-actions">
        ${p.id === cfg.active_persona ? '' : `<button class="btn ghost sm" data-action="ai-set-persona" data-id="${escapeHtml(p.id)}">使用</button>`}
        <button class="btn ghost sm" data-action="ai-edit-persona" data-id="${escapeHtml(p.id)}">编辑</button>
        <button class="btn ghost sm" data-action="ai-del-persona" data-id="${escapeHtml(p.id)}">删除</button>
      </div>
    </div>`).join('');

  return `
  <div class="card">
    <div class="card-head">
      <div class="card-title">人设</div>
      <button class="btn ghost sm" data-action="ai-add-persona">新建人设</button>
    </div>
    ${rows || '<div class="empty-hint">还没有人设。人设就是一段系统提示词，决定 AI 的语气和边界。</div>'}
    ${state.ai.personaForm ? aiPersonaForm(state.ai.personaForm) : ''}
  </div>`;
}

function aiPersonaForm(form) {
  return `
  <div class="ai-form">
    <div class="ai-form-row"><label>名称</label><input id="ai-persona-name" value="${escapeHtml(form.name || '')}" placeholder="例如 群助手 / 猫娘"></div>
    <div class="ai-form-row"><label>提示词</label>
      <textarea id="ai-persona-prompt" rows="6" placeholder="你是一个……">${escapeHtml(form.prompt || '')}</textarea>
    </div>
    <div class="ai-form-actions">
      <button class="btn primary sm" data-action="ai-save-persona">保存</button>
      <button class="btn ghost sm" data-action="ai-cancel-form">取消</button>
    </div>
  </div>`;
}

function aiMemorySection(cfg, memories) {
  const rows = memories.map(m => `
    <div class="ai-row">
      <div class="ai-row-main">
        <div class="ai-row-sub">${escapeHtml(m.content)}</div>
        <div class="ai-row-sub" style="opacity:.6">${escapeHtml(m.created_at || '')} · ${m.source === 'auto' ? '自动' : '手动'}</div>
      </div>
      <div class="ai-row-actions">
        <button class="btn ghost sm" data-action="ai-del-memory" data-id="${escapeHtml(m.id)}">删除</button>
      </div>
    </div>`).join('');

  return `
  <div class="card">
    <div class="card-head">
      <div class="card-title">记忆</div>
      <div style="display:flex;gap:8px;align-items:center">
        <label class="ai-switch"><input type="checkbox" id="ai-memory-enabled" ${cfg.memory_enabled ? 'checked' : ''} data-change="ai-toggle-memory"> 启用</label>
        <button class="btn ghost sm" data-action="ai-clear-memory">清空</button>
      </div>
    </div>
    <div class="ai-form-row"><label>新增</label>
      <input id="ai-memory-input" placeholder="例如：用户偏好简短回答，不喜欢表情包">
      <button class="btn primary sm" data-action="ai-add-memory">添加</button>
    </div>
    ${rows || '<div class="empty-hint">还没有记忆。记忆会在每次对话时拼进系统提示词。</div>'}
  </div>`;
}

function aiChatSection(ai) {
  const bubbles = ai.chat.map(m => `
    <div class="ai-bubble ${m.role === 'user' ? 'ai-bubble-user' : 'ai-bubble-ai'} ${m.error ? 'ai-bubble-err' : ''}">
      ${m.role === 'user' ? escapeHtml(m.content) : aiRenderMarkdown(m.content)}
    </div>`).join('');

  return `
  <div class="card">
    <div class="card-head">
      <div class="card-title">对话测试</div>
      <div style="display:flex;gap:8px;align-items:center">
        <label class="ai-switch" title="打开后会把工具清单写进提示词；当前版本不会真的执行工具">
          <input type="checkbox" id="ai-tools-enabled" ${(ai.config && ai.config.tool_calling_enabled) ? 'checked' : ''} data-change="ai-toggle-tools"> 工具能力
        </label>
        <button class="btn ghost sm" data-action="ai-clear-chat">清空对话</button>
      </div>
    </div>
    <div class="ai-chat">
      ${bubbles || '<div class="empty-hint">发一条消息试试。回复里的 ``` 代码块会被渲染成代码样式。</div>'}
    </div>
    <div class="ai-chat-input">
      <textarea id="ai-chat-input" rows="2" placeholder="输入消息，Ctrl+Enter 发送"></textarea>
      <button class="btn primary" data-action="ai-send" ${ai.busy ? 'disabled' : ''}>${ai.busy ? '发送中…' : '发送'}</button>
    </div>
  </div>`;
}

/* ============================================================================
 * 自动回复网关
 * ========================================================================== */

const AI_GATEWAY_DEFAULTS = {
  enabled: false,
  auto_start: false,
  ws_url: 'ws://127.0.0.1:3001',
  http_url: 'http://127.0.0.1:3000',
  access_token: '',
  reply_private: true,
  reply_group_at: true,
  context_rounds: 10,
  cooldown_ms: 1500,
  max_reply_chars: 800,
  save_stickers: true,
  send_stickers: true,
};

function aiGatewaySection(cfg) {
  const g = Object.assign({}, AI_GATEWAY_DEFAULTS, cfg.gateway || {});
  const st = state.ai.gatewayStatus || {};
  const running = !!st.running;

  return `
  <div class="card">
    <div class="card-head">
      <div class="card-title">自动回复</div>
      <div style="display:flex;gap:8px;align-items:center">
        ${running
          ? '<span class="status-pill" style="background:var(--ok-soft);color:var(--ok)"><span class="dot ok"></span>运行中</span>'
          : '<span class="status-pill" style="background:var(--bg-elevated);color:var(--text-3)"><span class="dot"></span>未运行</span>'}
        <button class="btn ${running ? 'ghost' : 'primary'} sm" data-action="ai-gw-toggle">${running ? '停止网关' : '启动网关'}</button>
      </div>
    </div>
    <div class="ai-form">
      <div class="ai-form-row"><label>开关</label>
        <label class="ai-switch"><input type="checkbox" id="ai-gw-enabled" ${g.enabled ? 'checked' : ''}> 启用自动回复</label>
        <label class="ai-switch"><input type="checkbox" id="ai-gw-autostart" ${g.auto_start ? 'checked' : ''}> 桌面端启动时自动运行</label>
      </div>
      <div class="ai-form-row"><label>回复范围</label>
        <label class="ai-switch"><input type="checkbox" id="ai-gw-private" ${g.reply_private ? 'checked' : ''}> 私聊</label>
        <label class="ai-switch"><input type="checkbox" id="ai-gw-groupat" ${g.reply_group_at ? 'checked' : ''}> 群聊仅 @ 时</label>
      </div>
      <div class="ai-form-row"><label>WS 地址</label><input id="ai-gw-ws" value="${escapeHtml(g.ws_url)}"></div>
      <div class="ai-form-row"><label>HTTP 地址</label><input id="ai-gw-http" value="${escapeHtml(g.http_url)}"></div>
      <div class="ai-form-row"><label>OneBot Token</label><input id="ai-gw-token" type="password" value="${escapeHtml(g.access_token)}"></div>
      <div class="ai-form-row"><label>上下文轮数</label><input id="ai-gw-rounds" type="number" min="1" max="100" value="${g.context_rounds}">
        <label>冷却 ms</label><input id="ai-gw-cooldown" type="number" min="0" value="${g.cooldown_ms}" style="max-width:120px">
        <label>最大字符</label><input id="ai-gw-maxchars" type="number" min="50" max="5000" value="${g.max_reply_chars}" style="max-width:120px">
      </div>
      <div class="ai-form-row"><label>表情包</label>
        <label class="ai-switch"><input type="checkbox" id="ai-gw-savesticker" ${g.save_stickers ? 'checked' : ''}> 保存收到的图片到本地</label>
        <label class="ai-switch"><input type="checkbox" id="ai-gw-sendsticker" ${g.send_stickers ? 'checked' : ''}> 允许 AI 用 [sticker:名字] 发送</label>
      </div>
      <div class="ai-form-actions">
        <button class="btn primary sm" data-action="ai-gw-save">保存配置</button>
        <span class="ai-hint">上下文存 ai/contexts/&lt;会话&gt;.json，表情存 ai/stickers/，都在本机</span>
      </div>
    </div>
  </div>`;
}
