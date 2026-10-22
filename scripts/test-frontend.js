/* 前端纯函数测试：不依赖浏览器，用 node 直接跑。
 * 运行：node scripts/test-frontend.js
 *
 * 目前覆盖 aiRenderMarkdown —— 它把 AI 回复转成 HTML，
 * 既是 XSS 面（必须转义），也是代码块格式的唯一实现。
 */
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const root = path.resolve(__dirname, '..');
const aiSrc = fs.readFileSync(path.join(root, 'frontend/scripts/views/ai.js'), 'utf8');

// 极简 escapeHtml 桩，与 frontend/scripts/core/dom.js 的行为保持一致
function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

const sandbox = { escapeHtml, console };
vm.createContext(sandbox);
vm.runInContext(aiSrc, sandbox);
const render = sandbox.aiRenderMarkdown;

let pass = 0;
let fail = 0;

function check(name, cond, detail) {
  if (cond) {
    pass++;
    console.log('  ok   ' + name);
  } else {
    fail++;
    console.log('  FAIL ' + name + (detail ? ' -> ' + detail : ''));
  }
}

console.log('aiRenderMarkdown');

// 1. 代码块
const fenced = render('说明\n```python\na = 1\nb = 2\n```\n结束');
check('包围代码块生成 pre.ai-code', fenced.includes('<pre class="ai-code">'));
check('代码块保留换行', fenced.includes('a = 1\nb = 2'));
check('代码块外的换行转 br', fenced.includes('<br>'));
check('代码块内不做 br', !fenced.includes('a = 1<br>'));

// 2. 行内代码
const inline = render('用 `npm i` 安装');
check('行内代码渲染', inline.includes('<code class="ai-inline">npm i</code>'));

// 3. 粗体
check('粗体渲染', render('**重点**').includes('<strong>重点</strong>'));

// 4. XSS：尖括号必须被转义
const evil = render('<img src=x onerror=alert(1)>');
check('HTML 被转义', !evil.includes('<img') && evil.includes('&lt;img'));

// 5. XSS：代码块内同样转义
const evilCode = render('```html\n<script>alert(1)</script>\n```');
check('代码块内 HTML 被转义', !evilCode.includes('<script>') && evilCode.includes('&lt;script&gt;'));

// 6. 空输入
check('空字符串安全', render('') === '' && render(null) === '');

console.log('\n' + pass + ' passed, ' + fail + ' failed');
process.exit(fail === 0 ? 0 : 1);
