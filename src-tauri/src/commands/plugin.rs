// 由 main.rs 拆分生成：插件管理命令
// 该模块的职责见 docs/ARCHITECTURE.md。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::models::*;
use crate::services::*;
use crate::state::*;
use crate::utils::*;

/// 扫描给定插件目录下的一级子目录，返回插件列表。
///
/// 过滤规则：
///   - 只看一级子目录，不递归；
///   - 跳过以 . 开头的隐藏目录（.git 之类）；
///   - 目录名以 .disabled 结尾视为"禁用"，但返回给前端时 dir 字段是去掉后缀的干净名字；
///   - 尝试读 <dir>/package.json，把 name/version/description/author 抽出来；
///     读不到就用目录名当 name，其他字段空字符串；超过 1 MiB 的跳过。
///   - has_exec = 是否存在 index.js / main.js / main.py。
#[tauri::command]
pub fn list_plugins(path: String) -> Vec<PluginInfo> {
    let root = Path::new(&path);
    let mut out = Vec::new();
    if !root.is_dir() { return out; }
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() { continue; }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if name.starts_with('.') { continue; }
            let enabled = !name.ends_with(".disabled");
            let clean = name.trim_end_matches(".disabled").to_string();

            let mut plugin_name = clean.clone();
            let mut version = String::new();
            let mut desc = String::new();
            let mut author = String::new();
            // 先查元数据长度再决定读不读：超限就跳过，避免 read_to_string 把超大文件
            // 整个读进内存（既慢又可能吃满内存）。
            let pkg = p.join("package.json");
            let too_big = fs::metadata(&pkg).map(|m| m.len() > MAX_PACKAGE_JSON_BYTES).unwrap_or(false);
            if !too_big {
                if let Ok(txt) = fs::read_to_string(&pkg) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                        // 显示名优先级：displayName（社区模板常用）→ plugin（NapCat 官方插件的
                        // 显示名字段，例：{"name":"napcat-plugin-cleaner","plugin":"缓存清理"}）
                        // → name。少了 plugin 这一档，官方插件在列表里只会显示成目录名。
                        if let Some(s) = v.get("displayName").and_then(|x| x.as_str()) { plugin_name = s.to_string(); }
                        else if let Some(s) = v.get("plugin").and_then(|x| x.as_str()) { plugin_name = s.to_string(); }
                        else if let Some(s) = v.get("name").and_then(|x| x.as_str()) { plugin_name = s.to_string(); }
                        if let Some(s) = v.get("version").and_then(|x| x.as_str()) { version = s.to_string(); }
                        if let Some(s) = v.get("description").and_then(|x| x.as_str()) { desc = s.to_string(); }
                        match v.get("author") {
                            Some(serde_json::Value::String(s)) => author = s.clone(),
                            Some(serde_json::Value::Object(o)) => {
                                if let Some(s) = o.get("name").and_then(|x| x.as_str()) { author = s.to_string(); }
                            }
                            _ => {}
                        }
                    }
                }
            }
            let has_exec = p.join("index.js").exists()
                || p.join("main.js").exists()
                || p.join("main.py").exists();

            out.push(PluginInfo { dir: clean, name: plugin_name, version, desc, author, enabled, has_exec });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 安装插件：把拖入的文件/文件夹复制到 plugins_dir。
///
/// - 如果是文件夹：整目录复制到 plugins_dir/<目录名>
/// - 如果是 .zip：用 PowerShell Expand-Archive 解压到 plugins_dir/<zip名去掉.zip>
/// - 其他文件类型：拒绝（插件必须是文件夹或 zip）
///
/// 目标目录必须显式校验（ensure_plugins_dir）：配置未加载时 resolve_plugins_dir
/// 返回空串，PathBuf::from("") 会变成相对路径，把插件复制到进程当前目录去。
#[tauri::command]
pub fn install_plugin(
    app: tauri::AppHandle, state: tauri::State<Arc<AppState>>, source: String,
) -> Result<String, String> {
    let _ = app; // 保留参数签名（前端按名调用），当前实现不需要 AppHandle
    let st = state.inner().clone();
    let plugins_dir = {
        let cfg = lock(&st.config);
        resolve_plugins_dir(&cfg.napcat_path)
    };
    let plugins_dir = ensure_plugins_dir(&plugins_dir)?;
    fs::create_dir_all(&plugins_dir).map_err(|e| format!("创建 plugins 目录失败: {e}"))?;

    let src = Path::new(&source);
    if !src.exists() { return Err(format!("源路径不存在: {source}")); }

    if src.is_dir() {
        let dir_name = src.file_name().and_then(|s| s.to_str()).unwrap_or("plugin").to_string();
        let dest = plugins_dir.join(&dir_name);
        if dest.exists() { return Err(format!("插件目录已存在: {dir_name}")); }
        copy_dir_recursive(src, &dest, 0).map_err(|e| format!("复制文件夹失败: {e}"))?;
        Ok(format!("已安装插件: {dir_name}"))
    } else if let Some(ext) = src.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("zip") {
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin").to_string();
            let dest = plugins_dir.join(&stem);
            if dest.exists() { return Err(format!("插件目录已存在: {stem}")); }
            // 复用 expand_zip：与一键安装走同一条解压路径，避免两处各写一遍 Expand-Archive
            expand_zip(src, &dest)?;
            Ok(format!("已解压插件: {stem}"))
        } else {
            Err(format!("不支持的文件类型 .{ext}，请拖入文件夹或 .zip 文件"))
        }
    } else {
        Err("无法识别的文件类型".to_string())
    }
}

/// 从 URL 一键安装插件：下载 zip → 解压到 plugins_dir/<name> → 展平单层目录。
///
/// 为什么用 `curl.exe` 而不是纯 Rust：本项目没有 http 客户端（也不值得为"装插件"这一个
/// 功能引入 reqwest + TLS 栈），而 Windows 10+ 自带 curl.exe，**且它能跟随 302 重定向**——
/// 这一点是关键：`github.com/.../releases/download/...` 会 302 到
/// `objects.githubusercontent.com`。实测本机 github.com 直连 21 秒超时，而 objects 域名可达，
/// 所以必须靠重定向才能落到能下载的域名上。
/// 用户在「设置 → 代理」里填了 proxy 时会通过 `--proxy` 交给 curl。
///
/// 错误信息刻意写清楚"下一步能做什么"（配代理 / 手动下载后拖放），因为这类失败几乎都是网络环境导致。
///
/// **为什么是 async**：同步 command 会在 Tauri 主线程上跑，而这里最长可能等 300 秒（curl --max-time），
/// 那会把整个界面冻住（连窗口都拖不动）。声明成 async 后它会跑在 async runtime 的工作线程上，
/// 期间前端可以正常显示"正在下载…"，UI 不受影响。
#[tauri::command]
pub async fn install_plugin_from_url(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>, url: String, name: String,
) -> Result<String, String> {
    validate_plugin_url(&url)?;
    // 目录名白名单：只允许字母数字与 . _ -，杜绝 ../ 或绝对路径造成的路径穿越
    let name = name.trim().to_string();
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!("插件名不合法: {name}"));
    }
    let st = state.inner().clone();
    let (plugins_dir, proxy) = {
        let cfg = lock(&st.config);
        (resolve_plugins_dir(&cfg.napcat_path), cfg.proxy.clone())
    };
    let plugins_dir = ensure_plugins_dir(&plugins_dir)?;
    fs::create_dir_all(&plugins_dir).map_err(|e| format!("创建 plugins 目录失败: {e}"))?;

    let dest = plugins_dir.join(&name);
    if dest.exists() { return Err(format!("插件目录已存在: {name}")); }
    // 临时文件放插件目录同盘：既避免跨盘复制，也不往系统临时目录（C 盘）丢东西
    let zip = plugins_dir.join(format!(".{name}.download.zip"));
    let _ = fs::remove_file(&zip);

    // ---- 下载：直连失败自动切镜像 ----
    let mut candidates: Vec<String> = vec![url.clone()];
    if url.starts_with("https://github.com/") {
        for m in GITHUB_MIRRORS {
            candidates.push(format!("{m}{url}"));
        }
    }
    let mut last_err: Option<String> = None;
    let mut used = String::new();
    let mut downloaded = false;
    for cand in &candidates {
        match download_via_curl(cand, &zip, &proxy) {
            Ok(()) => { downloaded = true; used = cand.clone(); break; }
            Err(e) => { last_err = Some(e); let _ = fs::remove_file(&zip); }
        }
    }
    if !downloaded {
        let e = last_err.unwrap_or_else(|| "未知原因".to_string());
        let mirrors = candidates.len() - 1;
        // 落盘一份：toast 只会显示几秒，而网络类失败最需要事后排查
        write_error_log(&app, "err", &format!(
            "[plugin-install] {name} 下载失败（直连 + {mirrors} 个镜像都失败）url={url} err={e}"
        ));
        return Err(format!(
            "下载失败：{e}；已尝试直连与 {mirrors} 个公共镜像。\
             如果都不通，请在「设置 → 代理」填入可用代理后重试，\
             或先在浏览器下载该 zip 再拖到插件页的虚线框安装。"
        ));
    }
    if used != url {
        write_error_log(&app, "warn", &format!("[plugin-install] {name} 直连失败，已改用镜像下载: {used}"));
    }

    // ---- 解压 → 展平 → 就位 ----
    let tmp = plugins_dir.join(format!(".{name}.extract"));
    let _ = fs::remove_dir_all(&tmp);
    if let Err(e) = expand_zip(&zip, &tmp) {
        let _ = fs::remove_file(&zip);
        let _ = fs::remove_dir_all(&tmp);
        write_error_log(&app, "err", &format!("[plugin-install] {name} 解压失败: {e}"));
        return Err(e);
    }
    let _ = fs::remove_file(&zip);
    if let Err(e) = flatten_single_dir(&tmp) {
        let _ = fs::remove_dir_all(&tmp);
        write_error_log(&app, "err", &format!("[plugin-install] {name} 展平失败: {e}"));
        return Err(e);
    }
    if let Err(e) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_dir_all(&tmp);
        let msg = format!("移动解压结果失败: {e}");
        write_error_log(&app, "err", &format!("[plugin-install] {name}: {msg}"));
        return Err(msg);
    }
    write_error_log(&app, "warn", &format!("[plugin-install] {name} 安装成功 url={url}"));
    Ok(format!("已安装插件: {name}"))
}
