//! 场景采集：唤出轮盘时记录「用户当时在看什么」。
//! - 最前面的应用、窗口标题、浏览器链接、文档路径（标记、收藏用）
//! - 光标（即问号标记）位置下的文字、附近段落、选中文字、网页正文（问号用，帮助模型理解用户关心的点）
//!
//! 采集在唤出时开始：命中测试（光标下是什么元素）必须在轮盘窗口出现前做完，
//! 其余部分（AppleScript 读浏览器等较慢）在后台继续，执行动作时再取结果。

use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Scene {
    pub app_name: String,
    pub app_path: Option<String>,
    pub bundle_id: Option<String>,
    pub pid: Option<i32>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub doc_path: Option<String>,
    /// 唤出点（全局坐标，点）
    pub anchor: (f64, f64),
    /// 用户选中的文字
    pub selection: Option<String>,
    /// 标记正下方元素的文字
    pub focus_text: Option<String>,
    /// 标记附近的段落
    pub nearby_text: Option<String>,
    /// 页面正文节选（网页）
    pub page_text: Option<String>,
}

impl Scene {
    /// 归组用的来源键：同一链接 / 同一文件 / 同一窗口的内容放在一起。
    pub fn source_key(&self) -> String {
        if let Some(u) = &self.url {
            return normalize_url(u);
        }
        if let Some(p) = &self.doc_path {
            return format!("file:{p}");
        }
        format!(
            "app:{}:{}",
            self.bundle_id.as_deref().unwrap_or(&self.app_name),
            self.window_title.as_deref().unwrap_or("")
        )
    }

    /// 一句话标题。
    pub fn headline(&self) -> String {
        self.window_title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| self.app_name.clone())
    }

    /// 发给后端的场景信息。marker 为标记在全屏截图中的位置比例。
    pub fn context_json(&self, marker: Option<(f64, f64)>) -> Value {
        let mut v = json!({
            "app": self.app_name,
            "title": self.window_title,
            "url": self.url,
            "selection": self.selection,
            "focus_text": self.focus_text,
            "nearby_text": self.nearby_text,
            "page_text": self.page_text,
        });
        if let Some((x, y)) = marker {
            v["marker"] = json!({ "x": x, "y": y });
        }
        if self.url.is_none() {
            if let Some(p) = &self.doc_path {
                v["title"] = json!(format!("{}（文件 {p}）", self.headline()));
            }
        }
        v
    }
}

/// 清洗窗口标题：去掉零宽字符（如飞书文档的隐形水印）、
/// 浏览器附加的「内存用量高」提示和末尾的应用名。
pub fn clean_title(raw: &str, app_name: &str) -> String {
    let mut t = strip_invisible(raw);
    let suffix = format!(" - {app_name}");
    if !app_name.is_empty() && t.ends_with(&suffix) {
        t.truncate(t.len() - suffix.len());
    }
    // Chrome：「标题 - 内存用量高 - 1.2 GB」
    for marker in [" - 内存用量高", " - High memory usage"] {
        if let Some(i) = t.find(marker) {
            t.truncate(i);
        }
    }
    t.trim().to_string()
}

fn strip_invisible(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'))
        .collect()
}

/// 整理采集到的文字：去隐形字符、压缩空白、截断。
fn tidy(s: &str, max: usize) -> Option<String> {
    let s = strip_invisible(s);
    let mut out = String::new();
    let mut blank = 0;
    for line in s.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            if blank == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank = 0;
        out.push_str(&line);
        out.push('\n');
    }
    let out = out.trim();
    if out.is_empty() {
        return None;
    }
    Some(if out.chars().count() > max { out.chars().take(max).collect() } else { out.to_string() })
}

/// 去掉锚点和常见跟踪参数，让同一篇文章的链接归为一组。
pub fn normalize_url(raw: &str) -> String {
    let Ok(mut u) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    u.set_fragment(None);
    let kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !(k.starts_with("utm_") || ["spm", "from", "share_source", "share_medium", "ref", "fbclid", "gclid"].contains(&k.as_str()))
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(kept);
    }
    let s = u.to_string();
    s.trim_end_matches('/').to_string()
}

// ---------- 后台采集 ----------

static PENDING: Mutex<Option<JoinHandle<Scene>>> = Mutex::new(None);
/// 命中测试最多等这么久再让轮盘出现
const HIT_TEST_WAIT: Duration = Duration::from_millis(150);

/// 唤出轮盘时、轮盘窗口出现前调用：在 (x, y) 处开始采集当前场景。
/// 会阻塞到命中测试完成（最多 150ms），避免读到轮盘窗口自己。
pub fn begin(x: f64, y: f64) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let t = Instant::now();
        let s = capture_scene(x, y, tx);
        log::info!(
            "场景：采集完成，耗时 {}ms，应用「{}」，窗口「{}」，链接 {}，文件 {}，选中 {} 字，标记处 {} 字，附近 {} 字，正文 {} 字",
            t.elapsed().as_millis(),
            s.app_name,
            s.window_title.as_deref().unwrap_or("-"),
            s.url.as_deref().unwrap_or("-"),
            s.doc_path.as_deref().unwrap_or("-"),
            count(&s.selection),
            count(&s.focus_text),
            count(&s.nearby_text),
            count(&s.page_text),
        );
        s
    });
    *PENDING.lock().unwrap() = Some(handle);
    if rx.recv_timeout(HIT_TEST_WAIT).is_err() {
        log::warn!("场景：命中测试超过 {}ms，先显示轮盘", HIT_TEST_WAIT.as_millis());
    }
}

fn count(s: &Option<String>) -> usize {
    s.as_ref().map(|t| t.chars().count()).unwrap_or(0)
}

/// 取本次唤出时采集的场景（必要时等它完成）。
pub fn take() -> Scene {
    let handle = PENDING.lock().unwrap().take();
    match handle {
        Some(h) => h.join().unwrap_or_default(),
        None => Scene::default(),
    }
}

/// 运行外部命令，超时则结束它。stdin 可选。
fn run_with_timeout(cmd: &mut Command, stdin: Option<&str>, timeout: Duration) -> Result<String, String> {
    use std::io::Write;
    let mut child = cmd
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(input.as_bytes());
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                return Err(format!("超时（{}ms）", timeout.as_millis()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn capture_scene(x: f64, y: f64, hit_done: mpsc::Sender<()>) -> Scene {
    let mut s = frontmost_app().unwrap_or_default();
    s.anchor = (x, y);

    // 1. 命中测试：光标下的元素及附近文字（必须在轮盘出现前完成）
    if let Some(pid) = s.pid {
        if s.bundle_id.as_deref().is_some_and(is_chromium_like) {
            // Chrome / Electron 默认不向辅助功能暴露网页内容，需要先打开
            ax::enable_manual_accessibility(pid);
        }
    }
    let hit = ax::hit_test(x, y);
    let _ = hit_done.send(());
    s.focus_text = hit.focus.and_then(|t| tidy(&t, 1500));
    s.nearby_text = hit.nearby.and_then(|t| tidy(&t, 6000));
    s.selection = ax::selected_text().and_then(|t| tidy(&t, 4000));

    // 2. 窗口标题、文档路径
    if let Some(pid) = s.pid {
        let (title, doc) = ax::focused_window_info(pid);
        s.window_title = title.map(|t| clean_title(&t, &s.app_name)).filter(|t| !t.is_empty());
        s.doc_path = doc;
    }

    // 3. 浏览器：链接，以及（若允许）用 JavaScript 读网页上下文，比辅助功能更准
    if let Some(bid) = s.bundle_id.clone() {
        s.url = browser::url(&bid);
        if s.url.is_some() {
            if let Some(page) = browser::page_context(&bid, x, y) {
                if page.focus.is_some() {
                    s.focus_text = page.focus.and_then(|t| tidy(&t, 1500));
                }
                if page.nearby.is_some() {
                    s.nearby_text = page.nearby.and_then(|t| tidy(&t, 6000));
                }
                if page.selection.is_some() {
                    s.selection = page.selection.and_then(|t| tidy(&t, 4000));
                }
                s.page_text = page.page.and_then(|t| tidy(&t, 12000));
            }
        }
    }
    s
}

#[cfg(not(target_os = "macos"))]
fn capture_scene(x: f64, y: f64, hit_done: mpsc::Sender<()>) -> Scene {
    let _ = hit_done.send(());
    Scene { app_name: "未知应用".into(), anchor: (x, y), ..Default::default() }
}

#[cfg(target_os = "macos")]
fn is_chromium_like(bid: &str) -> bool {
    browser::CHROMIUM.contains(&bid)
        || ["com.microsoft.VSCode", "com.tinyspeck.slackmacgap", "com.electron", "notion.id", "com.figma.Desktop", "com.bytedance.lark", "com.bytedance.macos.feishu"]
            .iter()
            .any(|p| bid.starts_with(p))
}

/// 用 lsappinfo 读最前面的应用（约 10ms，无需权限）。
#[cfg(target_os = "macos")]
fn frontmost_app() -> Option<Scene> {
    let asn = run_with_timeout(Command::new("lsappinfo").arg("front"), None, Duration::from_secs(1)).ok()?;
    let info = run_with_timeout(
        Command::new("lsappinfo").args(["info", "-only", "name", "-only", "pid", "-only", "bundleid", "-only", "bundlepath", &asn]),
        None,
        Duration::from_secs(1),
    )
    .ok()?;
    Some(parse_lsappinfo(&info))
}

fn parse_lsappinfo(info: &str) -> Scene {
    let mut s = Scene::default();
    let quoted = |line: &str| line.split_once('"').and_then(|(_, r)| r.rsplit_once('"')).map(|(v, _)| v.to_string());
    for (i, line) in info.lines().enumerate() {
        let line = line.trim();
        if i == 0 {
            if let Some(name) = line.strip_prefix('"').and_then(|r| r.split_once('"')).map(|(n, _)| n) {
                s.app_name = name.to_string();
            }
        } else if line.starts_with("bundleID=") {
            s.bundle_id = quoted(line);
        } else if line.starts_with("bundle path=") {
            s.app_path = quoted(line);
        } else if let Some(rest) = line.strip_prefix("pid = ") {
            s.pid = rest.split_whitespace().next().and_then(|p| p.parse().ok());
        }
    }
    s
}

/// 浏览器：AppleScript 读链接；JavaScript 读网页上下文（需在浏览器里允许 Apple 事件中的 JavaScript）。
#[cfg(target_os = "macos")]
mod browser {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub const CHROMIUM: &[&str] = &[
        "com.google.Chrome",
        "com.google.Chrome.canary",
        "com.google.Chrome.beta",
        "com.microsoft.edgemac",
        "com.brave.Browser",
        "company.thebrowser.Browser",
        "org.chromium.Chromium",
        "com.vivaldi.Vivaldi",
        "com.operasoftware.Opera",
    ];
    const SAFARI: &[&str] = &["com.apple.Safari", "com.apple.SafariTechnologyPreview"];

    static JS_HINTED: AtomicBool = AtomicBool::new(false);

    fn osascript(script: &str, timeout: Duration) -> Result<String, String> {
        run_with_timeout(Command::new("osascript").arg("-"), Some(script), timeout)
    }

    pub fn url(bid: &str) -> Option<String> {
        let script = if CHROMIUM.contains(&bid) {
            format!(r#"tell application id "{bid}" to return URL of active tab of front window"#)
        } else if SAFARI.contains(&bid) {
            format!(r#"tell application id "{bid}" to return URL of front document"#)
        } else {
            return None;
        };
        match osascript(&script, Duration::from_secs(2)) {
            Ok(u) if u.starts_with("http") || u.starts_with("file:") => Some(u),
            Ok(_) => None,
            Err(e) => {
                log::warn!("场景：读取浏览器链接失败（{bid}），可能需要在「隐私与安全性 → 自动化」里允许：{e}");
                None
            }
        }
    }

    pub struct Page {
        pub selection: Option<String>,
        pub focus: Option<String>,
        pub nearby: Option<String>,
        pub page: Option<String>,
    }

    /// 在网页里取：选中文字、光标下元素的文字、所在段落及前后段落、标题结构与正文节选。
    const JS: &str = r#"(function(sx,sy){function T(s,n){s=String(s||'').replace(/[ \t]+/g,' ').replace(/\n{3,}/g,'\n\n').trim();return s.length>n?s.slice(0,n):s}var x=sx-window.screenX,y=sy-window.screenY-(window.outerHeight-window.innerHeight);var el=document.elementFromPoint(x,y),focus='',near='';if(el){focus=T(el.innerText||el.textContent,1500);var B=/^(P|LI|TD|TH|PRE|BLOCKQUOTE|H[1-6]|DD|DT|FIGCAPTION|SECTION|ARTICLE|TABLE|UL|OL|DIV)$/,b=el;while(b&&b!==document.body&&(!B.test(b.tagName)||T(b.innerText,9999).length<120))b=b.parentElement;if(b&&b!==document.body){var parts=[],p=b.previousElementSibling,i=0;while(p&&i<2){parts.unshift(T(p.innerText,800));p=p.previousElementSibling;i++}parts.push(T(b.innerText,3000));var n=b.nextElementSibling;i=0;while(n&&i<2){parts.push(T(n.innerText,800));n=n.nextElementSibling;i++}near=parts.filter(Boolean).join('\n\n')}}var root=document.querySelector('article,main,[role=main]')||document.body;var heads=[].slice.call(document.querySelectorAll('h1,h2,h3')).slice(0,30).map(function(h){return h.tagName+' '+T(h.innerText,120)}).join('\n');var m=document.querySelector('meta[name=description]');var page=(m&&m.content?'摘要：'+m.content+'\n\n':'')+(heads?'标题结构：\n'+heads+'\n\n':'')+'正文：\n'+T(root.innerText,12000);return JSON.stringify({sel:T(String(window.getSelection()),4000),focus:focus,near:T(near,6000),page:page})})(SX,SY)"#;

    pub fn page_context(bid: &str, x: f64, y: f64) -> Option<Page> {
        let js = JS.replace("(SX,SY)", &format!("({},{})", x.round(), y.round()));
        // AppleScript 字符串转义
        let js = js.replace('\\', "\\\\").replace('"', "\\\"");
        let script = if CHROMIUM.contains(&bid) {
            format!(r#"tell application id "{bid}" to execute active tab of front window javascript "{js}""#)
        } else if SAFARI.contains(&bid) {
            format!(r#"tell application id "{bid}" to do JavaScript "{js}" in front document"#)
        } else {
            return None;
        };
        let out = match osascript(&script, Duration::from_millis(2500)) {
            Ok(o) => o,
            Err(e) => {
                if !JS_HINTED.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "场景：无法用 JavaScript 读取网页内容（{bid}），问号只能用截图和辅助功能文字。\
                         Chrome 可在菜单「显示 → 开发者 → 允许 Apple 事件中的 JavaScript」开启，\
                         Safari 在「开发 → 允许 Apple 事件中的 JavaScript」：{e}"
                    );
                }
                return None;
            }
        };
        let v: Value = serde_json::from_str(&out).ok()?;
        let s = |k: &str| v[k].as_str().map(str::to_string).filter(|t| !t.trim().is_empty());
        Some(Page { selection: s("sel"), focus: s("focus"), nearby: s("near"), page: s("page") })
    }
}

/// 辅助功能 API：命中测试、附近文字、选中文字、窗口标题和文档路径。
#[cfg(target_os = "macos")]
mod ax {
    use std::ffi::c_void;

    use core_foundation::base::{CFGetTypeID, CFRelease, CFRetain, CFTypeRef, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::{CFString, CFStringRef};

    type AXUIElementRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> i32;
        fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> i32;
        fn AXUIElementCopyElementAtPosition(el: AXUIElementRef, x: f32, y: f32, out: *mut AXUIElementRef) -> i32;
        fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> i32;
        fn AXUIElementGetTypeID() -> usize;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(arr: CFTypeRef) -> isize;
        fn CFArrayGetValueAtIndex(arr: CFTypeRef, i: isize) -> CFTypeRef;
        fn CFArrayGetTypeID() -> usize;
    }

    /// 持有一个 AXUIElement（析构时释放）。
    struct El(AXUIElementRef);
    impl Drop for El {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) };
            }
        }
    }

    impl El {
        fn attr(&self, name: &str) -> Option<CFTypeRef> {
            let attr = CFString::new(name);
            let mut value: CFTypeRef = std::ptr::null();
            let err = unsafe { AXUIElementCopyAttributeValue(self.0, attr.as_concrete_TypeRef(), &mut value) };
            (err == 0 && !value.is_null()).then_some(value)
        }

        fn string(&self, name: &str) -> Option<String> {
            let v = self.attr(name)?;
            unsafe {
                if CFGetTypeID(v) != CFString::type_id() {
                    CFRelease(v);
                    return None;
                }
                Some(CFString::wrap_under_create_rule(v as CFStringRef).to_string())
            }
        }

        fn element(&self, name: &str) -> Option<El> {
            let v = self.attr(name)?;
            unsafe {
                if CFGetTypeID(v) != AXUIElementGetTypeID() {
                    CFRelease(v);
                    return None;
                }
            }
            Some(El(v))
        }

        fn children(&self) -> Vec<El> {
            let Some(arr) = self.attr("AXChildren") else { return vec![] };
            let mut out = vec![];
            unsafe {
                if CFGetTypeID(arr) == CFArrayGetTypeID() {
                    let n = CFArrayGetCount(arr).min(200);
                    for i in 0..n {
                        let item = CFArrayGetValueAtIndex(arr, i);
                        if !item.is_null() && CFGetTypeID(item) == AXUIElementGetTypeID() {
                            CFRetain(item);
                            out.push(El(item));
                        }
                    }
                }
                CFRelease(arr);
            }
            out
        }

        fn role(&self) -> String {
            self.string("AXRole").unwrap_or_default()
        }

        /// 元素自身的文字：值、标题、描述（去重）。
        fn own_text(&self) -> String {
            let mut parts: Vec<String> = vec![];
            for k in ["AXValue", "AXTitle", "AXDescription"] {
                if let Some(t) = self.string(k) {
                    let t = t.trim().to_string();
                    if !t.is_empty() && !parts.contains(&t) && !is_internal_name(&t) {
                        parts.push(t);
                    }
                }
            }
            parts.join(" ")
        }

        /// 深度优先收集子树文字，限制节点数和字数。
        fn subtree_text(&self, budget_nodes: &mut usize, out: &mut String, max_chars: usize) {
            if *budget_nodes == 0 || out.len() >= max_chars {
                return;
            }
            *budget_nodes -= 1;
            let t = self.own_text();
            if !t.is_empty() {
                out.push_str(&t);
                out.push('\n');
            }
            for c in self.children() {
                c.subtree_text(budget_nodes, out, max_chars);
            }
        }
    }

    /// 过滤控件的内部类名（如 WatermarkContentView），它们对理解内容没有帮助。
    fn is_internal_name(t: &str) -> bool {
        !t.contains(' ')
            && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && ["View", "Controller", "Window", "Cell", "Layer"].iter().any(|s| t.ends_with(s))
            && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
    }

    fn system_wide() -> El {
        let el = El(unsafe { AXUIElementCreateSystemWide() });
        unsafe { AXUIElementSetMessagingTimeout(el.0, 0.25) };
        el
    }

    /// 让 Chrome / Electron 应用向辅助功能暴露网页内容。
    pub fn enable_manual_accessibility(pid: i32) {
        unsafe {
            let app = El(AXUIElementCreateApplication(pid));
            AXUIElementSetMessagingTimeout(app.0, 0.2);
            let attr = CFString::new("AXManualAccessibility");
            AXUIElementSetAttributeValue(app.0, attr.as_concrete_TypeRef(), CFBoolean::true_value().as_CFTypeRef());
        }
    }

    pub struct Hit {
        pub focus: Option<String>,
        pub nearby: Option<String>,
    }

    /// 命中测试：(x, y) 处元素的文字，以及向上找到的足够大的容器里的文字。
    pub fn hit_test(x: f64, y: f64) -> Hit {
        let sys = system_wide();
        let mut raw: AXUIElementRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyElementAtPosition(sys.0, x as f32, y as f32, &mut raw) };
        if err != 0 || raw.is_null() {
            return Hit { focus: None, nearby: None };
        }
        let el = El(raw);

        // 标记处文字：自身没有就向上找最近有文字的祖先
        let mut focus = el.own_text();
        let mut cur: Option<El> = None;
        for _ in 0..3 {
            if !focus.is_empty() {
                break;
            }
            let next = cur.as_ref().unwrap_or(&el).element("AXParent");
            match next {
                Some(p) => {
                    focus = p.own_text();
                    cur = Some(p);
                }
                None => break,
            }
        }

        // 附近内容：逐级向上，直到容器文字足够多；不越过网页根 / 窗口
        let mut nearby = String::new();
        let mut node = el.element("AXParent");
        for _ in 0..8 {
            let Some(n) = node else { break };
            let role = n.role();
            if role == "AXWindow" || role == "AXApplication" {
                break;
            }
            let mut text = String::new();
            let mut budget = 400;
            n.subtree_text(&mut budget, &mut text, 6000);
            nearby = text;
            if nearby.chars().count() >= 400 || role == "AXWebArea" {
                break;
            }
            node = n.element("AXParent");
        }

        Hit {
            focus: (!focus.trim().is_empty()).then_some(focus),
            nearby: (!nearby.trim().is_empty()).then_some(nearby),
        }
    }

    /// 当前焦点元素里选中的文字。
    pub fn selected_text() -> Option<String> {
        let sys = system_wide();
        let focused = sys.element("AXFocusedUIElement")?;
        focused.string("AXSelectedText").filter(|t| !t.trim().is_empty())
    }

    /// 返回（窗口标题，文档路径）。
    pub fn focused_window_info(pid: i32) -> (Option<String>, Option<String>) {
        let app = El(unsafe { AXUIElementCreateApplication(pid) });
        if app.0.is_null() {
            return (None, None);
        }
        unsafe { AXUIElementSetMessagingTimeout(app.0, 0.5) };
        let Some(win) = app.element("AXFocusedWindow").or_else(|| app.element("AXMainWindow")) else {
            return (None, None);
        };
        let title = win.string("AXTitle");
        let doc = win
            .string("AXDocument")
            .and_then(|u| url::Url::parse(&u).ok())
            .and_then(|u| u.to_file_path().ok())
            .map(|p| p.to_string_lossy().into_owned());
        (title, doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalization() {
        assert_eq!(
            normalize_url("https://example.com/a/b/?utm_source=x&id=3#sec"),
            "https://example.com/a/b/?id=3"
        );
        assert_eq!(normalize_url("https://example.com/post/#top"), "https://example.com/post");
    }

    #[test]
    fn title_cleaning() {
        let raw = "\u{200d}\u{200c}\u{2062}\u{feff}wan3.0-0831 - 飞书云文档 - 内存用量高 - 1.2 GB - Google Chrome";
        assert_eq!(clean_title(raw, "Google Chrome"), "wan3.0-0831 - 飞书云文档");
        assert_eq!(clean_title("README.md — ping-wheel", "Code"), "README.md — ping-wheel");
    }

    #[test]
    fn tidy_text() {
        assert_eq!(tidy("  a   b \n\n\n\n c\u{200b} ", 100).as_deref(), Some("a b\n\nc"));
        assert_eq!(tidy(" \n ", 100), None);
        assert_eq!(tidy("一二三四五", 3).as_deref(), Some("一二三"));
    }

    #[test]
    fn lsappinfo_parsing() {
        let info = "\"Google Chrome\" ASN:0x0-0x1: (in front) \n    bundleID=\"com.google.Chrome\"\n    bundle path=\"/Applications/Google Chrome.app\"\n    pid = 123 !cgsConnection";
        let s = parse_lsappinfo(info);
        assert_eq!(s.app_name, "Google Chrome");
        assert_eq!(s.bundle_id.as_deref(), Some("com.google.Chrome"));
        assert_eq!(s.app_path.as_deref(), Some("/Applications/Google Chrome.app"));
        assert_eq!(s.pid, Some(123));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod live_tests {
    use super::*;

    /// 真实采集当前光标处的场景，手动运行：cargo test scene_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn scene_live() {
        let (x, y) = crate::input_macos::cursor_location().unwrap_or((400.0, 400.0));
        begin(x, y);
        let s = take();
        println!("{s:#?}\nsource_key = {}", s.source_key());
        assert!(!s.app_name.is_empty());
    }
}
