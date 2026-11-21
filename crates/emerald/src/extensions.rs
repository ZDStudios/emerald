//! Extensions: store integration ("Add to Emerald"), the manager, toolbar pins and popups.
//!
//! * Chrome Web Store installs use Chrome's own installer (verified working in CEF Chrome
//!   style): Emerald only relabels the button. Extensions get their real ID and auto-update.
//! * Edge Add-ons: the Edge store's install API exists only inside Edge, so Emerald
//!   downloads the CRX from Microsoft's update service, verifies the signature
//!   (emerald_core::crx), asks the user, then loads it as an unpacked extension with its
//!   public key, keeping the same ID as in Edge.
//! * Management goes through `chrome.developerPrivate` / `chrome.management` on a hidden
//!   `chrome://extensions` page (the "service" browser), created on demand and closed when idle.
use crate::native::{self, Rect};
use crate::shell::{self, Shell};
use crate::util::{self, cs, later, opt_str, userfree};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use cef::*;
use emerald_core::session::Id;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Callback = Box<dyn FnOnce(&mut Shell, Result<Value, String>)>;

#[derive(Default)]
pub struct ExtState {
    pub pending_overlay: Option<(String, Value)>,
    pub pinned: Vec<String>,
    pub cache: Vec<Value>,
    svc: Option<Browser>,
    svc_creating: bool,
    svc_ready: bool,
    svc_last_use: u64,
    queue: Vec<(u64, String)>,
    pending: HashMap<u64, Callback>,
    next: u64,
    pub load_path: Option<PathBuf>,
    popup: Option<(Browser, Option<shell::Handle>)>,
    popup_rect: Rect,
    popup_creating: bool,
    pub edge_pending: Option<(String, PathBuf, String)>,
}

const SVC_PREFIX: &str = "\u{1}EMEXT:";

impl ExtState {
    pub fn pinned_json(&self) -> Value {
        json!(self.pinned.iter().filter_map(|id| self.cache.iter().find(|e| e["id"] == id.as_str() && e["enabled"] == true)).map(|e| json!({"id": e["id"], "name": e["name"], "icon": e["icon"], "popup": e["popup"]})).collect::<Vec<_>>())
    }
    pub fn shutdown(&mut self) {
        if let Some(b) = self.svc.take() { if let Some(h) = b.host() { h.close_browser(1); } }
        if let Some((b, _)) = self.popup.take() { if let Some(h) = b.host() { h.close_browser(1); } }
    }
}

fn state_file(s: &Shell) -> PathBuf { s.paths.file("extensions.json") }

pub fn load_state(s: &mut Shell) {
    let v: Value = emerald_core::store::load_json(&state_file(s));
    s.ext.pinned = v["pinned"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    s.ext.cache = v["cache"].as_array().cloned().unwrap_or_default();
}

fn save_state(s: &Shell) {
    // Cache (without icons) lets toolbar pins render before the service page has loaded.
    let _ = emerald_core::store::save_json(&state_file(s), &json!({"pinned": s.ext.pinned, "cache": s.ext.cache}));
}

// ====================================================================== service browser

fn svc_call(s: &mut Shell, expr: &str, cb: impl FnOnce(&mut Shell, Result<Value, String>) + 'static) {
    s.ext.next += 1;
    let id = s.ext.next;
    s.ext.pending.insert(id, Box::new(cb));
    let js = format!(
        "(async()=>{{let o;try{{o={{ok:true,r:await ({expr})}}}}catch(e){{o={{ok:false,e:String(e&&e.message||e)}}}}console.log({}+{id}+':'+JSON.stringify(o));}})()",
        serde_json::to_string(SVC_PREFIX).unwrap()
    );
    s.ext.queue.push((id, js));
    s.ext.svc_last_use = emerald_core::now_ms();
    ensure_svc(s);
    flush(s);
}

fn flush(s: &mut Shell) {
    if !s.ext.svc_ready { return; }
    let Some(b) = s.ext.svc.clone() else { return };
    for (_, js) in s.ext.queue.drain(..) { util::exec_js(&b, &js); }
}

fn dbg(m: &str) { if std::env::var("EMERALD_DEBUG").is_ok() { eprintln!("emerald[ext]: {m}"); } }

fn ensure_svc(s: &mut Shell) {
    dbg(&format!("ensure_svc svc={} creating={} ready={}", s.ext.svc.is_some(), s.ext.svc_creating, s.ext.svc_ready));
    if s.ext.svc.is_some() || s.ext.svc_creating || s.closing { return; }
    let Some(parent) = s.parent_handle() else { dbg("no parent"); return };
    s.ext.svc_creating = true;
    #[allow(unused_mut)]
    let mut wi = WindowInfo { runtime_style: RuntimeStyle::CHROME, ..Default::default() }.set_as_child(parent, &cef::Rect { x: 0, y: 0, width: 1, height: 1 });
    #[cfg(target_os = "windows")]
    { wi.style &= !windows_sys::Win32::UI::WindowsAndMessaging::WS_VISIBLE; }
    let mut client = SvcClient::new();
    let ok = browser_host_create_browser(Some(&wi), Some(&mut client), Some(&cs("chrome://extensions/")), Some(&BrowserSettings::default()), None, None);
    dbg(&format!("svc create returned {ok}"));
    // Close the service page after a minute of inactivity (it costs a renderer).
    fn idle_check() {
        shell::with(|s| {
            if s.ext.svc.is_some() && s.ext.pending.is_empty() && emerald_core::now_ms() - s.ext.svc_last_use > 60_000 {
                if let Some(b) = s.ext.svc.take() { if let Some(h) = b.host() { h.close_browser(1); } }
                s.ext.svc_ready = false;
            } else if s.ext.svc.is_some() || s.ext.svc_creating {
                later(30_000, idle_check);
            }
        });
    }
    later(30_000, idle_check);
}

wrap_life_span_handler! {
    pub struct SvcLife {}
    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser.cloned() else { return };
            dbg("svc created");
            shell::with(move |s| {
                if let Some(h) = b.host().map(|h| h.window_handle()) { native::place(h, None); }
                s.ext.svc = Some(b);
                s.ext.svc_creating = false;
            });
        }
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            shell::with(|s| { s.ext.svc = None; s.ext.svc_ready = false; s.ext.svc_creating = false; });
        }
    }
}

wrap_load_handler! {
    pub struct SvcLoad {}
    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: c_int) {
            if frame.is_none_or(|f| f.is_main() == 0) { return; }
            dbg("svc load_end");
            shell::with(|s| { s.ext.svc_ready = true; flush(s); });
        }
    }
}

wrap_display_handler! {
    pub struct SvcDisplay {}
    impl DisplayHandler {
        fn on_console_message(&self, _browser: Option<&mut Browser>, _level: LogSeverity, message: Option<&CefString>, _source: Option<&CefString>, _line: c_int) -> c_int {
            let msg = opt_str(message);
            dbg(&format!("svc console: {}", msg.chars().take(200).collect::<String>()));
            let Some(rest) = msg.strip_prefix(SVC_PREFIX) else { return 1 };
            let Some((id, json)) = rest.split_once(':') else { return 1 };
            let id: u64 = id.parse().unwrap_or(0);
            let v: Value = serde_json::from_str(json).unwrap_or_default();
            shell::with(move |s| {
                if let Some(cb) = s.ext.pending.remove(&id) {
                    let r = if v["ok"] == true { Ok(v["r"].clone()) } else { Err(v["e"].as_str().unwrap_or("error").to_string()) };
                    cb(s, r);
                }
            });
            1
        }
    }
}

wrap_dialog_handler! {
    pub struct SvcDialogs {}
    impl DialogHandler {
        fn on_file_dialog(&self, _browser: Option<&mut Browser>, _mode: FileDialogMode, _title: Option<&CefString>, _default_file_path: Option<&CefString>, _accept_filters: Option<&mut CefStringList>, _accept_extensions: Option<&mut CefStringList>, _accept_descriptions: Option<&mut CefStringList>, callback: Option<&mut FileDialogCallback>) -> c_int {
            // "Load unpacked" asks for a folder: answer with the folder Emerald prepared.
            let Some(cb) = callback else { return 0 };
            match shell::query(|s| s.ext.load_path.take()).flatten() {
                Some(p) => { let mut l = CefStringList::new(); l.append(&p.to_string_lossy()); cb.cont(Some(&mut l)); }
                None => cb.cancel(),
            }
            1
        }
    }
}

wrap_client! {
    pub struct SvcClient {}
    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(SvcLife::new()) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(SvcLoad::new()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(SvcDisplay::new()) }
        fn dialog_handler(&self) -> Option<DialogHandler> { Some(SvcDialogs::new()) }
    }
}

// ====================================================================== manifest helpers

/// Folder of an installed extension (store installs: <profile>/Extensions/<id>/<version>_0).
fn ext_dir(s: &Shell, info: &Value) -> Option<PathBuf> {
    let id = info["id"].as_str()?;
    let mine = s.paths.data().join("extensions").join(id);
    if mine.join("manifest.json").exists() { return Some(mine); }
    let p = info["path"].as_str().unwrap_or("");
    if !p.is_empty() {
        let p = if let Some(rest) = p.strip_prefix("~/") { dirs_home().join(rest) } else { PathBuf::from(p) };
        if p.join("manifest.json").exists() { return Some(p); }
    }
    let base = s.paths.engine_profile().join("Extensions").join(id);
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("manifest.json").exists()).collect();
    versions.sort();
    versions.pop()
}

fn dirs_home() -> PathBuf { PathBuf::from(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_default()) }

fn icon_data_url(dir: &Path, manifest: &Value) -> Option<String> {
    let pick = |v: &Value| -> Option<String> {
        if let Some(s) = v.as_str() { return Some(s.to_string()); }
        let obj = v.as_object()?;
        let mut sizes: Vec<(u32, String)> = obj.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string()))).collect();
        sizes.sort();
        sizes.iter().find(|(s, _)| *s >= 32).or(sizes.last()).map(|x| x.1.clone())
    };
    let rel = pick(&manifest["action"]["default_icon"]).or_else(|| pick(&manifest["browser_action"]["default_icon"])).or_else(|| pick(&manifest["icons"]))?;
    let rel = rel.trim_start_matches('/');
    if rel.contains("..") { return None; }
    let bytes = std::fs::read(dir.join(rel)).ok()?;
    let mime = if rel.ends_with(".svg") { "image/svg+xml" } else { "image/png" };
    Some(format!("data:{mime};base64,{}", B64.encode(bytes)))
}

fn read_manifest(dir: &Path) -> Value {
    std::fs::read_to_string(dir.join("manifest.json")).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or_default()
}

/// Resolve `__MSG_name__` from _locales (English, then default_locale).
fn localize(dir: &Path, manifest: &Value, text: &str) -> String {
    let Some(key) = text.strip_prefix("__MSG_").and_then(|t| t.strip_suffix("__")) else { return text.to_string() };
    for loc in ["en", "en_US", manifest["default_locale"].as_str().unwrap_or("en")] {
        let msgs: Value = std::fs::read_to_string(dir.join("_locales").join(loc).join("messages.json")).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or_default();
        if let Some(obj) = msgs.as_object() {
            if let Some((_, v)) = obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)) { if let Some(m) = v["message"].as_str() { return m.to_string(); } }
        }
    }
    text.to_string()
}

// ====================================================================== API

pub fn list_json(s: &Shell) -> Value { json!(s.ext.cache) }

/// Refresh the extension list from Chromium, then run `then`.
pub fn refresh(s: &mut Shell, then: impl FnOnce(&mut Shell) + 'static) {
    let expr = "chrome.developerPrivate.getExtensionsInfo({includeDisabled:true,includeTerminated:true}).then(l=>l.map(e=>({id:e.id,name:e.name,version:e.version,description:e.description,state:e.state,location:e.location,path:e.prettifiedPath||'',permissions:((e.permissions&&e.permissions.simplePermissions)||[]).map(p=>p.message),optionsUrl:e.optionsPage&&e.optionsPage.url||'',homeUrl:e.homePage&&e.homePage.url||'',type:e.type})))";
    svc_call(s, expr, move |s, r| {
        if let Ok(Value::Array(items)) = r {
            s.ext.cache = items.into_iter().filter(|e| e["type"] == "EXTENSION" || e["type"].is_null() || e["type"] == "extension").map(|mut e| {
                if let Some(dir) = ext_dir(s, &e) {
                    let m = read_manifest(&dir);
                    e["icon"] = icon_data_url(&dir, &m).map(Value::from).unwrap_or(Value::Null);
                    e["popup"] = m["action"]["default_popup"].as_str().or(m["browser_action"]["default_popup"].as_str()).map(Value::from).unwrap_or(Value::Null);
                    e["mv"] = m["manifest_version"].clone();
                    if let Some(n) = e["name"].as_str() { e["name"] = localize(&dir, &m, n).into(); }
                }
                e["enabled"] = json!(e["state"] == "ENABLED");
                e["pinned"] = json!(s.ext.pinned.iter().any(|p| e["id"] == p.as_str()));
                e
            }).collect();
            save_state(s);
            s.emit_all("extensions", json!(s.ext.cache));
            s.push_state();
        } else if let Err(e) = r {
            eprintln!("emerald: extension list failed: {e}");
        }
        then(s);
    });
}

pub fn action(s: &mut Shell, a: &Value) -> anyhow::Result<Value> {
    let id = a["id"].as_str().unwrap_or("").to_string();
    let valid_id = id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b));
    match a["do"].as_str().unwrap_or("") {
        "refresh" => refresh(s, |_| {}),
        "enable" | "disable" if valid_id => {
            let on = a["do"] == "enable";
            svc_call(s, &format!("chrome.management.setEnabled({},{on})", serde_json::to_string(&id)?), |s, r| { if let Err(e) = r { s.emit_ui("toast", json!({"text": format!("Couldn't change extension: {e}")})); } refresh(s, |_| {}); });
        }
        "remove" if valid_id => {
            svc_call(s, &format!("chrome.management.uninstall({},{{showConfirmDialog:false}})", serde_json::to_string(&id)?), move |s, r| {
                if let Err(e) = r { s.emit_ui("toast", json!({"text": format!("Couldn't remove: {e}")})); }
                s.ext.pinned.retain(|p| p != &id);
                let _ = std::fs::remove_dir_all(s.paths.data().join("extensions").join(&id));
                refresh(s, |_| {});
            });
        }
        "pin" if valid_id => {
            let pin = a["pinned"].as_bool().unwrap_or(true);
            s.ext.pinned.retain(|p| p != &id);
            if pin { s.ext.pinned.push(id); }
            for e in &mut s.ext.cache { e["pinned"] = json!(s.ext.pinned.iter().any(|p| e["id"] == p.as_str())); }
            save_state(s);
            s.push_state();
            s.emit_all("extensions", json!(s.ext.cache));
        }
        "options" => {
            if let Some(url) = a["url"].as_str().filter(|u| u.starts_with("chrome-extension://")) { let t = s.new_tab(Some(url), None, false); s.activate(t); }
        }
        "loadUnpacked" => {
            let p = PathBuf::from(a["path"].as_str().unwrap_or(""));
            anyhow::ensure!(p.join("manifest.json").exists(), "That folder has no manifest.json");
            load_unpacked(s, p);
        }
        "edgeConfirm" => {
            if let Some((_, dir, name)) = s.ext.edge_pending.take() {
                if a["accept"].as_bool() == Some(true) {
                    s.emit_ui("toast", json!({"text": format!("Adding {name}…")}));
                    load_unpacked(s, dir);
                } else {
                    let _ = std::fs::remove_dir_all(dir);
                }
            }
        }
        _ => anyhow::bail!("bad extension action"),
    }
    Ok(Value::Null)
}

fn load_unpacked(s: &mut Shell, dir: PathBuf) {
    s.ext.load_path = Some(dir);
    svc_call(s, "chrome.developerPrivate.updateProfileConfiguration({inDeveloperMode:true}).then(()=>chrome.developerPrivate.loadUnpacked({failQuietly:true,populateError:true}))", |s, r| {
        match r {
            Ok(v) if v.is_null() || v.get("error").is_none() => s.emit_ui("toast", json!({"text": "Extension added", "ms": 2500})),
            Ok(v) => s.emit_ui("toast", json!({"text": format!("Couldn't add extension: {}", v["error"].as_str().unwrap_or("unknown error")), "sticky": true})),
            Err(e) => s.emit_ui("toast", json!({"text": format!("Couldn't add extension: {e}"), "sticky": true})),
        }
        refresh(s, |_| {});
    });
}

// ====================================================================== stores

const CWS_JS: &str = include_str!("../../../ui/inject/store-chrome.js");
const EDGE_JS: &str = include_str!("../../../ui/inject/store-edge.js");

pub fn on_page_loaded(s: &mut Shell, _tab: Id, b: &Browser, url: &str) {
    if url.starts_with("https://chromewebstore.google.com/") {
        util::exec_js(b, CWS_JS);
        // A store install may have just happened; refresh the list shortly after.
        later(4000, || shell::with(|s| refresh(s, |_| {})));
    } else if url.starts_with("https://microsoftedge.microsoft.com/addons") {
        util::exec_js(b, EDGE_JS);
    }
    let _ = s;
}

/// Called from on_before_browse for `https://emerald.invalid/install?store=edge&id=…`.
/// Only honoured when the tab is currently on the Edge Add-ons site.
pub fn store_request(s: &mut Shell, tab: Id, url: &str) {
    let on_store = s.session.tab(tab).is_some_and(|t| t.url.starts_with("https://microsoftedge.microsoft.com/addons"));
    let q = url.split_once('?').map(|x| x.1).unwrap_or("");
    let params = emerald_core::url::parse_query(q);
    let get = |k: &str| params.iter().find(|(a, _)| a == k).map(|x| x.1.clone()).unwrap_or_default();
    let id = get("id");
    if !on_store || get("store") != "edge" || id.len() != 32 || !id.bytes().all(|b| (b'a'..=b'p').contains(&b)) {
        eprintln!("emerald: ignored install request {url}");
        return;
    }
    s.emit_ui("toast", json!({"text": "Downloading from Edge Add-ons…", "ms": 2500}));
    let crx_url = format!("https://edge.microsoft.com/extensionwebstorebase/v1/crx?response=redirect&prod=chromiumcrx&prodchannel=&x=id%3D{id}%26installsource%3Dondemand%26uc");
    download(&crx_url, move |bytes| shell::with(move |s| edge_downloaded(s, &id, bytes)));
}

fn edge_downloaded(s: &mut Shell, id: &str, bytes: Result<Vec<u8>, String>) {
    let r = bytes.map_err(anyhow::Error::msg).and_then(|b| {
        let crx = emerald_core::crx::parse(&b, Some(id))?;
        let dir = s.paths.data().join("extensions").join(id);
        let manifest = emerald_core::crx::unpack(&crx, &dir)?;
        Ok((dir, manifest))
    });
    match r {
        Ok((dir, m)) => {
            let name = localize(&dir, &m, m["name"].as_str().unwrap_or(id));
            let perms: Vec<String> = m["permissions"].as_array().into_iter().flatten().chain(m["host_permissions"].as_array().into_iter().flatten()).filter_map(|p| p.as_str().map(String::from)).collect();
            s.ext.edge_pending = Some((id.to_string(), dir, name.clone()));
            s.emit_ui("extConfirm", json!({"id": id, "name": name, "version": m["version"], "permissions": perms}));
        }
        Err(e) => s.emit_ui("toast", json!({"text": format!("Couldn't install from Edge Add-ons: {e}"), "sticky": true})),
    }
}

// ---------------------------------------------------------------- download via Chromium's network stack

wrap_urlrequest_client! {
    pub struct Fetch {
        buf: Arc<Mutex<Vec<u8>>>,
        done: Arc<Mutex<Option<Box<dyn FnOnce(Result<Vec<u8>, String>) + Send>>>>,
    }
    impl UrlrequestClient {
        fn on_download_data(&self, _request: Option<&mut Urlrequest>, data: *const u8, data_length: usize) {
            if !data.is_null() && data_length > 0 {
                let chunk = unsafe { std::slice::from_raw_parts(data, data_length) };
                let mut b = self.buf.lock().unwrap();
                if b.len() + chunk.len() <= 200 * 1024 * 1024 { b.extend_from_slice(chunk); }
            }
        }
        fn on_request_complete(&self, request: Option<&mut Urlrequest>) {
            let ok = request.as_ref().is_some_and(|r| r.request_status() == UrlrequestStatus::SUCCESS)
                && request.as_ref().and_then(|r| r.response()).is_some_and(|resp| resp.status() == 200);
            let data = std::mem::take(&mut *self.buf.lock().unwrap());
            if let Some(f) = self.done.lock().unwrap().take() {
                f(if ok { Ok(data) } else { Err("download failed".into()) });
            }
        }
    }
}

pub fn download(url: &str, done: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static) {
    let Some(mut req) = request_create() else { return };
    req.set_url(Some(&cs(url)));
    req.set_method(Some(&cs("GET")));
    let mut client = Fetch::new(Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Some(Box::new(done)))));
    let _ = urlrequest_create(Some(&mut req), Some(&mut client), None);
}

// ====================================================================== popups

wrap_life_span_handler! {
    pub struct PopupLife {}
    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser.cloned() else { return };
            shell::with(move |s| {
                let h = b.host().map(|h| h.window_handle());
                if let Some(h) = h { native::place(h, Some(s.ext.popup_rect)); native::raise(h); }
                if let Some(host) = b.host() { host.set_focus(1); }
                s.ext.popup = Some((b, h));
                s.ext.popup_creating = false;
            });
        }
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            shell::with(|s| { s.ext.popup = None; s.ext.popup_creating = false; });
        }
    }
}

wrap_keyboard_handler! {
    pub struct PopupKeys {}
    impl KeyboardHandler {
        fn on_pre_key_event(&self, _browser: Option<&mut Browser>, event: Option<&KeyEvent>, _os_event: crate::util::OsEvent<'_>, _is_keyboard_shortcut: Option<&mut c_int>) -> c_int {
            if event.is_some_and(|e| e.type_ == KeyEventType::RAWKEYDOWN && e.windows_key_code == 0x1B) {
                shell::with(close_popup);
                return 1;
            }
            0
        }
    }
}

wrap_client! {
    pub struct ExtPopupClient {}
    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(PopupLife::new()) }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> { Some(PopupKeys::new()) }
    }
}

pub fn close_popup(s: &mut Shell) {
    if let Some((b, h)) = s.ext.popup.take() {
        if let Some(h) = h { native::place(h, None); }
        if let Some(host) = b.host() { host.close_browser(1); }
    }
}

/// Open an extension's action popup under its toolbar icon (x, y in CSS px).
pub fn open_popup(s: &mut Shell, id: &str, x: f64, y: f64) {
    close_popup(s);
    let Some(e) = s.ext.cache.iter().find(|e| e["id"] == id) else { return };
    let Some(popup) = e["popup"].as_str() else {
        s.emit_ui("toast", json!({"text": format!("{} has no popup. It works in the background.", e["name"].as_str().unwrap_or("This extension"))}));
        return;
    };
    let Some(parent) = s.parent_handle() else { return };
    let d = s.dpr;
    let (w, h) = ((380.0 * d) as i32, (560.0 * d) as i32);
    let c = s.content;
    let rect = Rect { x: ((x * d) as i32 - w).max(c.x + 4).min(c.x + c.w - w - 4), y: (y * d) as i32 + 4, w, h: h.min(c.h - 8) };
    s.ext.popup_rect = rect;
    s.ext.popup_creating = true;
    let url = format!("chrome-extension://{id}/{}", popup.trim_start_matches('/'));
    let wi = WindowInfo { runtime_style: RuntimeStyle::CHROME, ..Default::default() }.set_as_child(parent, &rect.to_cef());
    let mut client = ExtPopupClient::new();
    browser_host_create_browser(Some(&wi), Some(&mut client), Some(&cs(&url)), Some(&BrowserSettings::default()), None, None);
    let _ = userfree;
}
