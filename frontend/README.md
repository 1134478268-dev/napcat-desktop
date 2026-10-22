# 前端开发约定

## 技术栈

零构建：原生 HTML + CSS + 全局 JS 脚本，没有打包器、没有框架、没有 npm 依赖。
Tauri WebView 直接加载 `frontend/index.html`，脚本在 `<body>` 末尾按顺序同步加载。

## 加载顺序（改动了要同步改 index.html）

```
core/state.js      → 全局 state 与常量
core/dom.js        → $ / escapeHtml / toast
ui/actions.js      → data-action 映射表
views/views.js     → 视图模板
core/router.js     → 路由与重绘
core/logs.js       → 日志渲染
ui/interactions.js → 业务交互
core/ipc.js        → invoke 封装
main.js            → 启动引导（最后）
```

依赖只能「向后看」：后面的文件可以调用前面定义的函数，反之不行。

## 约定

1. **视图纯函数**：`views.xxx()` 只返回 HTML 字符串，不碰 DOM。
2. **重绘唯一入口** `render()`，在 `core/router.js`。
3. **后端调用唯一入口** `callCmd(cmd, args)`，在 `core/ipc.js`；返回 `{ok, data, error}`，不抛出。
4. **事件委托**：按钮写 `data-action="name"`，处理函数注册在 `ui/actions.js` 的 `actions` 对象里。
5. **用户输入必须 `escapeHtml`** 后再拼进模板，防止模板注入。
6. 样式变量统一用 `styles/tokens.css` 里的 CSS 变量，不要写死颜色。

## 本地调试

改完直接刷新窗口（`F5` 或重启 debug 版）即可，不需要构建步骤。
若前端白屏：打开 WebView 开发者工具（debug 构建下 `Ctrl+Shift+I`）看 Console 报错，通常是脚本加载顺序或某文件语法错误。
