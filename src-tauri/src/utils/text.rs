// 由 main.rs 拆分生成：文本工具：脱敏、日志分级、二维码路径、base64
// 该模块的职责见 docs/ARCHITECTURE.md。



/// 从一行文本里提取 `marker` 后面的路径（到行尾）。
pub fn extract_after(line: &str, marker: &str) -> Option<String> {
    line.find(marker).map(|i| line[i+marker.len()..].trim().trim_matches(|c| c=='"' || c=='\'' || c=='`').to_string())
}
/// 从一行里找 `xxx\qrcode.png` 这种 Windows 路径（包含反斜杠）。
pub fn extract_after_path(line: &str, filename: &str) -> Option<String> {
    let idx = line.find(filename)?;
    // 向前找路径起点（盘符 X:\）
    let before = &line[..idx];
    let start = before.rfind(|c: char| c.is_alphabetic() && c==':').unwrap_or(0);
    let end = idx + filename.len();
    Some(line[start..end].to_string())
}
/// 从一行日志里提取二维码 PNG 的磁盘路径。
/// NapCat 不同打包形态都会打印这类行：中文"二维码已保存到 D:\...\qrcode.png"，
/// 或英文行里直接出现 qrcode.png。所以识别放在共用的日志转发里，不挂在某个形态的分支上。
pub fn find_qrcode_path(line: &str) -> Option<String> {
    if !line.contains("qrcode.png") && !line.contains("二维码已保存") { return None; }
    extract_after(line, "二维码已保存到").or_else(|| extract_after_path(line, "qrcode.png"))
}
/// 极简 base64 编码（不引入额外依赖，标准字母表）。
pub fn base64_encode(data: &[u8]) -> String {
    const T: &[u8;64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3)*4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len()>1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len()>2 { chunk[2] as u32 } else { 0 };
        let n = (b0<<16) | (b1<<8) | b2;
        s.push(T[((n>>18)&63) as usize] as char);
        s.push(T[((n>>12)&63) as usize] as char);
        s.push(if chunk.len()>1 { T[((n>>6)&63) as usize] as char } else { '=' });
        s.push(if chunk.len()>2 { T[(n&63) as usize] as char } else { '=' });
    }
    s
}

/// 把日志行里的敏感凭据脱敏后再推给前端。
///
/// 为什么要做：NapCat 的启动日志里经常会打印
///   `WebUI Local Panel Url: http://127.0.0.1:6099/webui?token=sk-xxxx`
///   `access_token=abcdef`
/// 这类 URL/query 串。前端日志视图是个普通 DOM 区域，未来如果有人把日志截图/录屏
/// 分享出去，token 就泄露了。这里做最朴素的字符串替换（不引 regex crate）：
/// token=xxx / access_token=xxx / authorization: xxx 一律替换为 ***。
///
/// === 为什么必须维护 search_from 游标（这是一个真实踩过的死循环）===
/// 被替换掉的是 key **后面**的值，key 本身还留在字符串里。如果每轮都从 0 开始
/// s.find(key)，下一轮会命中同一个 key，而此时 key 后面已经是 "***"（不含空白/&/引号），
/// end 会算成 3 → replace_range 把 "***" 换成 "***"，字符串不再变化 → 循环永不前进：
/// 日志线程 100% CPU 空转、后续日志全丢，管道写满还会连带拖住子进程。
/// 所以每轮都必须把游标推到替换点之后，保证单调前进；找不到 key、或 key 后面没有
/// 内容时同样要前进（否则一样原地打转）。
/// 需要脱敏的**键名**（大小写不敏感，所以统一写小写；**不含** `=`/`:` 等分隔符——
/// 分隔符由 redact 统一跳过，于是 `token=v`、`authorization: v`、`"token":"v"`
/// 三种写法共用同一个键名，新增敏感字段只需要在这里加一行）。
///
/// 为什么把 password 系列也放进来：NapCat 的 quick-login / 部署日志里会出现
/// `qq_password=...`，密码比 token 更不能出现在日志视图里（截图、录屏、导出都会带走）。
///
/// 已知不覆盖（如实保留为局限）：URL 编码形式（如 `token%3Dxxx`）、群消息体与 CQ 码原文；
/// 要彻底覆盖需要引入 regex crate，当前刻意不引。
pub const REDACT_KEYS: [&str; 12] = [
    // 通用 token / 授权
    "token", "access_token", "accesstoken", "access-token", "authorization",
    // 密码与密钥
    "password", "passwd", "qq_password", "api_key", "apikey", "secret", "x-api-key",
];

/// 在 `s[from..]` 里**不区分大小写**地查找 ASCII 键名，返回绝对字节偏移。
///
/// 为什么自己写：标准库没有"忽略大小写的子串查找"，而 `s.to_lowercase()` 会改变字节
/// 长度、让偏移量无法映射回原串。这里按**字节**比较：既避免 &str 切片落在多字节字符
/// （中文日志）中间 panic，也不会错位。键名都是 ASCII，比较是安全的。
pub fn find_ci(s: &str, needle: &str, from: usize) -> Option<usize> {
    let hay = s.as_bytes();
    let nee = needle.as_bytes();
    if nee.is_empty() || hay.len() < nee.len() { return None; }
    let mut i = from;
    while i + nee.len() <= hay.len() {
        if hay[i..i + nee.len()].eq_ignore_ascii_case(nee) { return Some(i); }
        i += 1;
    }
    None
}

pub fn redact(line: &str) -> String {
    let mut s = line.to_string();
    for key in REDACT_KEYS {
        // search_from 单调不减，每轮至少前进 key 的长度，所以这个循环一定终止。
        let mut search_from = 0usize;
        while let Some(idx) = find_ci(&s, key, search_from) {
            let after_key = idx + key.len();
            // 键名后面必须紧跟分隔符，否则它只是某个普通单词的一部分
            // （tokenizer / passwordless / secretary），跳过以免误脱敏正文。
            // 这里同样要推进游标，否则会在同一位置反复命中。
            let is_sep = after_key < s.len()
                && matches!(s.as_bytes()[after_key], b'=' | b':' | b'"' | b'\'' | b' ' | b'\t');
            if !is_sep {
                search_from = after_key;
                continue;
            }
            // 跳过键名与值之间的全部分隔符（`=` / `:` / `"` / `'` / 空白）：
            // 这样 `token=v`、`authorization: v`、`"token":"v"` 三种写法都能定位到值。
            let mut start = after_key;
            while start < s.len()
                && matches!(s.as_bytes()[start], b'=' | b':' | b'"' | b'\'' | b' ' | b'\t')
            { start += 1; }
            // HTTP 头风格 "Authorization: Bearer <token>" 里紧跟在 key 后面的是
            // **scheme 关键字**而不是凭据本体。若不跳过它，脱敏结果会是
            // "Authorization: *** xyz" —— 真正的 token 原样留在后面（已用等价的
            // Python 实现复现确认）。所以命中 Authorization: 时先连 scheme 一起跳过，
            // 再去脱敏它后面的值。
            // 比较用字节切片而不是 &str 切片：start 处可能是多字节字符（中文日志），
            // &str 切片落在字符中间会 panic。
            if key.starts_with("authorization") || key.starts_with("access-token") || key.starts_with("x-api-key") {
                for scheme in ["Bearer", "Basic", "Token", "Digest"] {
                    let matched = {
                        let sb = s.as_bytes();
                        sb.len() >= start + scheme.len()
                            && sb[start..start + scheme.len()].eq_ignore_ascii_case(scheme.as_bytes())
                    };
                    if matched {
                        start += scheme.len();
                        while start < s.len() && s.as_bytes()[start].is_ascii_whitespace() { start += 1; }
                        break;
                    }
                }
            }
            if start >= s.len() {
                // key 在行尾（后面只剩空白）：没有凭据可替换。
                // 但游标仍必须推到 key 之后，否则下一轮 find 又命中同一个 key —— 原地踏步。
                search_from = idx + key.len();
                continue;
            }
            let rest = &s[start..];
            // 值的终止位置：引号包裹的值取到配对引号（覆盖 token="abc" 与 JSON 的
            // "k":"v"），否则取到空白 / & / 引号 / 行尾。
            let end = if let Some(stripped) = rest.strip_prefix('"') {
                stripped.find('"').map(|i| i + 2).unwrap_or(rest.len())
            } else if let Some(stripped) = rest.strip_prefix('\'') {
                stripped.find('\'').map(|i| i + 2).unwrap_or(rest.len())
            } else {
                rest.find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == '\'')
                    .unwrap_or(rest.len())
            };
            // 即使 end == 0（值紧邻空白/&/引号）也是一次插入，字符串因此变长，
            // 配合下面的游标推进同样不会死循环。
            s.replace_range(start..start + end, "***");
            // 关键：跳过刚写入的 "***"，保证下一轮不会重新命中同一个 key。
            search_from = start + 3;
        }
    }
    s
}

/// 把子进程一行 stdout/stderr 粗略分类成前端日志视图能高亮的级别。
/// 规则是启发式的：只看关键词，不保证 100% 准确，目标是让 ERROR 红、WARN 黄、
/// RECV 蓝这些样式能自然命中。匹配优先级（先命中先返回，顺序很重要）：
///   err(error/failed/失败/exception/panic) → warn(warn/警告/延迟)
///   → plugin(plugin/插件/[plugin，前端特殊着色) → recv(收到消息/recv/message，蓝色高亮)
///   → ok(登录成功/已监听/connected/ok) → info
/// level 取值集合是对前端的契约，只能是这 6 个之一。
pub fn classify_log(line: &str) -> &'static str {
    let l = line.to_lowercase();
    if l.contains("error") || l.contains("failed") || line.contains("失败")
        || l.contains("exception") || l.contains("panic") {
        "err"
    } else if l.contains("warn") || line.contains("警告") || line.contains("延迟") {
        "warn"
    } else if l.contains("plugin") || line.contains("插件") {
        // "[plugin" 也含 "plugin" 子串，一并命中
        "plugin"
    } else if line.contains("收到消息") || l.contains("recv") || l.contains("message") {
        "recv"
    } else if line.contains("登录成功") || line.contains("已监听") || l.contains("connected") || l.contains("ok") {
        "ok"
    } else {
        "info"
    }
}

// ===========================================================================
// NapCat 子进程启动（首启与自动重启共用同一条路径）
// ===========================================================================
