/**
 * NapCat AI 网关
 *
 * 由桌面端 spawn 的独立 Node 进程（标准输出被桌面端的日志转发链路捕获）。
 * 职责：
 *   1. 连 NapCat 的正向 WebSocket，收消息事件
 *   2. 判断要不要回（私聊 / 群里被 @）
 *   3. 读该会话的本地上下文 + 人设 + 记忆，拼 prompt 调 AI
 *   4. 把回复发回 QQ
 *   5. 保存对话上下文、下载收到的图片到本地表情库
 *
 * 为什么用 Node 而不是塞进 Rust：
 *   Rust 侧要加 WS 客户端依赖（tungstenite 之类），而从国内网络拉 crates 不稳；
 *   项目的「整合包形态」本来就有 node.exe。代价是多一个进程，好处是崩了不连累面板。
 *
 * 环境变量（由桌面端注入）：
 *   NAPCAT_AI_DIR   ai 数据目录（含 config.json / personas.json / memories.json）
 */
import fs from 'node:fs';
import path from 'node:path';

const AI_DIR = process.env.NAPCAT_AI_DIR || path.join(process.cwd(), 'ai');
const CFG_FILE = path.join(AI_DIR, 'config.json');
const PERSONA_FILE = path.join(AI_DIR, 'personas.json');
const MEMORY_FILE = path.join(AI_DIR, 'memories.json');
const CTX_DIR = path.join(AI_DIR, 'contexts');
const STICKER_DIR = path.join(AI_DIR, 'stickers');
const STICKER_INDEX = path.join(STICKER_DIR, 'index.json');

let cfg = null;
let gateway = null;
let provider = null;
let persona = null;
let memories = [];

const lastReplyAt = new Map();
let selfId = '';

function log(msg) {
  process.stdout.write('[ai-gateway] ' + msg + '\n');
}

function logErr(msg) {
  process.stderr.write('[ai-gateway] ' + msg + '\n');
}

function readJson(file, fallback) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return fallback;
  }
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = file + '.tmp';
  fs.writeFileSync(tmp, JSON.stringify(value, null, 2), 'utf8');
  fs.renameSync(tmp, file);
}

function loadConfig() {
  cfg = readJson(CFG_FILE, null);
  if (!cfg) throw new Error('读不到 AI 配置：' + CFG_FILE);

  gateway = Object.assign(
    {
      enabled: false,
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
    },
    cfg.gateway || {}
  );

  provider = (cfg.providers || []).find((p) => p.id === cfg.active_provider) || (cfg.providers || [])[0];
  if (!provider) throw new Error('没有可用的 AI 服务，请先在 AI 页配置');

  const personas = readJson(PERSONA_FILE, []);
  persona = personas.find((p) => p.id === cfg.active_persona) || null;
  memories = readJson(MEMORY_FILE, []);
}

/* ---------------- 上下文 ---------------- */

function ctxKey(type, id) {
  return type + '_' + id;
}

function loadContext(key) {
  return readJson(path.join(CTX_DIR, key + '.json'), { key, messages: [] });
}

function saveContext(key, ctx) {
  const limit = Math.max(2, (gateway.context_rounds || 10) * 2);
  if (ctx.messages.length > limit) {
    ctx.messages = ctx.messages.slice(ctx.messages.length - limit);
  }
  ctx.updated_at = new Date().toISOString();
  writeJson(path.join(CTX_DIR, key + '.json'), ctx);
}

/* ---------------- prompt ---------------- */

function listStickerNames() {
  return Object.keys(readJson(STICKER_INDEX, {}));
}

function buildSystemPrompt() {
  const parts = [];
  if (persona && persona.prompt && persona.prompt.trim()) parts.push(persona.prompt.trim());
  if (cfg.system_extra && cfg.system_extra.trim()) parts.push(cfg.system_extra.trim());

  if (cfg.memory_enabled && memories.length) {
    const limit = Math.max(1, cfg.memory_limit || 50);
    const recent = memories.slice(-limit);
    parts.push('你记得以下关于用户的事实：\n' + recent.map((m) => '- ' + m.content).join('\n'));
  }

  if (gateway.send_stickers) {
    const names = listStickerNames().slice(0, 40);
    parts.push(
      '你可以在回复里使用本地表情：写 [sticker:名字] 即可发送该表情。' +
        (names.length ? '可用表情：' + names.join('、') + '。' : '当前还没有本地表情。') +
        '回复要口语化、简短，像真人聊天。'
    );
  }
  return parts.join('\n\n');
}

/* ---------------- 表情库 ---------------- */

/** 允许下载图片的域名后缀。消息里的 image.url 是**外部可控数据**：
 *  不限制的话，一条恶意消息就能让网关去请求内网地址（SSRF），
 *  或者指向一个超大文件把磁盘写满。这里只放行 QQ 官方图片 CDN。 */
const STICKER_HOST_ALLOW = ['qpic.cn', 'qq.com', 'gtimg.cn', 'qlogo.cn'];
/** 单张表情的大小上限（5 MB）。超出直接丢弃，不落盘。 */
const MAX_STICKER_BYTES = 5 * 1024 * 1024;

function isAllowedImageUrl(raw) {
  let u;
  try {
    u = new URL(raw);
  } catch {
    return false;
  }
  if (u.protocol !== 'https:' && u.protocol !== 'http:') return false;
  const host = u.hostname.toLowerCase();
  return STICKER_HOST_ALLOW.some((h) => host === h || host.endsWith('.' + h));
}

function saveStickerFromMessage(seg) {
  if (!gateway.save_stickers) return;
  try {
    const url = seg && seg.data ? seg.data.url : null;
    const file = seg && seg.data ? seg.data.file : null;
    if (!url && !file) return;

    fs.mkdirSync(STICKER_DIR, { recursive: true });
    const stamp = Date.now();
    const m = String(file || url).match(/\.(png|jpe?g|gif|webp)/i);
    const ext = m ? m[1].toLowerCase() : 'png';
    const name = 'auto_' + stamp;
    const target = path.join(STICKER_DIR, name + '.' + ext);

    if (typeof url !== 'string' || !/^https?:/i.test(url)) return;
    if (!isAllowedImageUrl(url)) {
      logErr('跳过非 QQ CDN 的图片地址（防 SSRF）: ' + String(url).slice(0, 80));
      return;
    }

    fetch(url, { signal: AbortSignal.timeout(15000) })
      .then((r) => {
        if (!r.ok) throw new Error('HTTP ' + r.status);
        const declared = Number(r.headers.get('content-length') || 0);
        if (declared > MAX_STICKER_BYTES) throw new Error('图片超过 5MB');
        return r.arrayBuffer();
      })
      .then((buf) => {
        if (buf.byteLength > MAX_STICKER_BYTES) throw new Error('图片超过 5MB');
        fs.writeFileSync(target, Buffer.from(buf));
        const idx = readJson(STICKER_INDEX, {});
        idx[name] = name + '.' + ext;
        writeJson(STICKER_INDEX, idx);
        log('已保存表情 ' + name + '.' + ext);
      })
      .catch((e) => logErr('保存表情失败: ' + e.message));
  } catch (e) {
    logErr('保存表情异常: ' + e.message);
  }
}

function stickerPath(name) {
  const idx = readJson(STICKER_INDEX, {});
  const file = idx[name];
  if (!file) return null;
  if (file.includes('/') || file.includes('\\') || file.includes('..')) return null;
  const p = path.join(STICKER_DIR, file);
  return fs.existsSync(p) ? p : null;
}

/* ---------------- AI 调用 ---------------- */

function chatUrl(p) {
  const base = p.api_base.replace(/\/+$/, '');
  if (p.kind === 'anthropic') return base.endsWith('/messages') ? base : base + '/messages';
  if (p.kind === 'gemini') {
    if (base.includes(':generateContent')) return base;
    return base + '/models/' + p.model + ':generateContent';
  }
  return base.endsWith('/chat/completions') ? base : base + '/chat/completions';
}

function buildBody(kind, system, messages) {
  if (kind === 'anthropic') {
    return {
      model: provider.model,
      system,
      max_tokens: cfg.max_tokens || 2048,
      temperature: cfg.temperature ?? 0.7,
      messages: messages.filter((m) => m.role !== 'system'),
    };
  }
  if (kind === 'gemini') {
    return {
      systemInstruction: { parts: [{ text: system }] },
      contents: messages
        .filter((m) => m.role !== 'system')
        .map((m) => ({
          role: m.role === 'assistant' ? 'model' : 'user',
          parts: [{ text: m.content }],
        })),
      generationConfig: {
        temperature: cfg.temperature ?? 0.7,
        maxOutputTokens: cfg.max_tokens || 2048,
      },
    };
  }
  return {
    model: provider.model,
    messages: [{ role: 'system', content: system }, ...messages],
    temperature: cfg.temperature ?? 0.7,
    max_tokens: cfg.max_tokens || 2048,
  };
}

function parseReply(kind, v) {
  if (kind === 'anthropic') {
    return (v.content || []).filter((c) => c.type === 'text').map((c) => c.text).join('');
  }
  if (kind === 'gemini') {
    return ((((v.candidates || [])[0] || {}).content || {}).parts || []).map((p) => p.text || '').join('');
  }
  return (((v.choices || [])[0] || {}).message || {}).content || '';
}

async function callAi(system, messages) {
  const headers = { 'Content-Type': 'application/json' };
  if (provider.kind === 'anthropic') {
    headers['x-api-key'] = provider.api_key;
    headers['anthropic-version'] = '2023-06-01';
  } else if (provider.kind === 'gemini') {
    headers['x-goog-api-key'] = provider.api_key;
  } else {
    headers.Authorization = 'Bearer ' + provider.api_key;
  }

  const res = await fetch(chatUrl(provider), {
    method: 'POST',
    headers,
    body: JSON.stringify(buildBody(provider.kind, system, messages)),
    signal: AbortSignal.timeout(120000),
  });
  const text = await res.text();
  if (!res.ok) throw new Error('HTTP ' + res.status + ': ' + text.slice(0, 300));
  return parseReply(provider.kind, JSON.parse(text));
}

/* ---------------- OneBot 发送 ---------------- */

async function callApi(action, payload) {
  const res = await fetch(gateway.http_url.replace(/\/+$/, '') + '/' + action, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: 'Bearer ' + gateway.access_token,
    },
    body: JSON.stringify(payload),
    signal: AbortSignal.timeout(30000),
  });
  if (!res.ok) throw new Error(action + ' HTTP ' + res.status);
  return res.json().catch(() => ({}));
}

async function sendReply(ctx, text) {
  const segments = [];
  const stickerRe = /\[sticker:([^\]]+)\]/g;
  let m;
  while ((m = stickerRe.exec(text)) !== null) {
    const p = stickerPath(m[1].trim());
    if (p) segments.push({ type: 'image', data: { file: 'file:///' + p.replace(/\\/g, '/') } });
  }
  const plain = text.replace(stickerRe, '').trim();
  if (plain) segments.push({ type: 'text', data: { text: plain.slice(0, gateway.max_reply_chars || 800) } });
  if (!segments.length) return;

  if (ctx.type === 'private') {
    await callApi('send_private_msg', { user_id: Number(ctx.id), message: segments });
  } else {
    await callApi('send_group_msg', { group_id: Number(ctx.id), message: segments });
  }
  log('已回复 ' + ctx.type + '_' + ctx.id + '（' + segments.length + ' 段）');
}

/* ---------------- 消息处理 ---------------- */

function isAtMe(segments) {
  return segments.some((s) => s.type === 'at' && String(s.data ? s.data.qq : '') === String(selfId));
}

function extractText(segments) {
  return segments
    .filter((s) => s.type === 'text')
    .map((s) => (s.data ? s.data.text || '' : ''))
    .join('')
    .trim();
}

function handleMessage(evt) {
  const type = evt.message_type;
  if (type !== 'private' && type !== 'group') return;
  if (String(evt.user_id) === String(selfId)) return;

  const segments = Array.isArray(evt.message) ? evt.message : [];
  const text = extractText(segments);

  for (const seg of segments) {
    if (seg.type === 'image') saveStickerFromMessage(seg);
  }

  if (!text) return;
  if (type === 'private' && !gateway.reply_private) return;
  if (type === 'group' && gateway.reply_group_at && !isAtMe(segments)) return;

  const key = ctxKey(type, type === 'private' ? evt.user_id : evt.group_id);
  const now = Date.now();
  if (now - (lastReplyAt.get(key) || 0) < (gateway.cooldown_ms || 0)) {
    log('冷却中，跳过 ' + key);
    return;
  }
  lastReplyAt.set(key, now);

  const ctx = { type, id: type === 'private' ? evt.user_id : evt.group_id };
  const store = loadContext(key);
  const system = buildSystemPrompt();
  const speaker = type === 'group' ? (evt.sender ? evt.sender.nickname : '群友') : '用户';
  const messages = [...store.messages, { role: 'user', content: speaker + '：' + text }];

  log('收到 ' + key + '：' + text.slice(0, 60));

  callAi(system, messages)
    .then((reply) => {
      if (!reply || !reply.trim()) return;
      store.messages.push({ role: 'user', content: text });
      store.messages.push({ role: 'assistant', content: reply });
      saveContext(key, store);
      return sendReply(ctx, reply);
    })
    .catch((e) => logErr('回复失败 ' + key + ': ' + e.message));
}

/* ---------------- 连接 ---------------- */

function connect() {
  /* WS 也要带 token：NapCat 的 websocketServers 配了 token 时，
   * 客户端握手必须带 ?access_token=...，否则会被直接拒绝（表现为反复 TIME_WAIT）。 */
  let url = gateway.ws_url;
  if (gateway.access_token) {
    url += (url.includes('?') ? '&' : '?') + 'access_token=' + encodeURIComponent(gateway.access_token);
  }
  log('连接 ' + gateway.ws_url + (gateway.access_token ? '（带 token）' : '（无 token）'));

  const ws = new WebSocket(url);

  ws.addEventListener('open', () => log('WebSocket 已连接'));
  ws.addEventListener('message', (e) => {
    let evt;
    try {
      evt = JSON.parse(typeof e.data === 'string' ? e.data : e.data.toString());
    } catch {
      return;
    }
    if (evt.post_type === 'meta_event' && evt.meta_event_type === 'lifecycle') {
      selfId = String(evt.self_id || '');
      log('账号已就绪 self_id=' + selfId);
      return;
    }
    if (evt.post_type === 'message') {
      try {
        handleMessage(evt);
      } catch (err) {
        logErr('处理消息异常: ' + err.message);
      }
    }
  });
  ws.addEventListener('error', () => logErr('WebSocket 错误（NapCat 是否已启动？）'));
  ws.addEventListener('close', () => {
    log('WebSocket 断开，3 秒后重连');
    setTimeout(connect, 3000);
  });
}

/* ---------------- 启动 ---------------- */

try {
  loadConfig();
  if (!gateway.enabled) {
    log('自动回复未启用（AI 页打开开关后再启动）');
    process.exit(0);
  }
  if (!provider.api_key || !provider.model || !provider.api_base) {
    logErr('AI 服务配置不完整（key / model / api_base）');
    process.exit(2);
  }
  fs.mkdirSync(CTX_DIR, { recursive: true });
  log('provider=' + provider.name + ' model=' + provider.model + ' persona=' + (persona ? persona.name : '无'));
  log('私聊=' + gateway.reply_private + ' 群@=' + gateway.reply_group_at +
      ' 上下文轮数=' + gateway.context_rounds + ' 表情保存=' + gateway.save_stickers);
  connect();
} catch (e) {
  logErr('启动失败: ' + e.message);
  process.exit(1);
}
