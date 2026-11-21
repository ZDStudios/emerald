//! Built-in privacy: a lightweight ad/tracker blocker, DNT/GPC headers, and per-site
//! permissions (quiet by default: notification prompts are denied unless the user allowed
//! the site).
//!
//! The blocker is intentionally small: a curated set of ad/tracker hosts matched by domain
//! suffix (no cosmetic filtering). It costs well under 1 MB of RAM. For full EasyList
//! coverage, install uBlock Origin Lite, which works in Emerald (see FEATURES.md).
use crate::shell;
use crate::util::{cs, opt_str, userfree};
use cef::*;
use emerald_core::url::host;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

static ENABLED: AtomicBool = AtomicBool::new(true);
static DNT: AtomicBool = AtomicBool::new(true);
pub static BLOCKED: AtomicU64 = AtomicU64::new(0);
static LIST: OnceLock<HashSet<&'static str>> = OnceLock::new();

const TRACKERS: &str = include_str!("../../../ui/data/trackers.txt");

fn list() -> &'static HashSet<&'static str> {
    LIST.get_or_init(|| TRACKERS.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect())
}

pub fn configure(block: bool, dnt: bool) {
    ENABLED.store(block, Ordering::Relaxed);
    DNT.store(dnt, Ordering::Relaxed);
}

/// Is `url`'s host (or any parent domain) on the tracker list?
pub fn is_tracker(url: &str) -> bool {
    let Some(h) = host(url) else { return false };
    let l = list();
    let mut d = h.as_str();
    loop {
        if l.contains(d) { return true; }
        match d.find('.') { Some(i) => d = &d[i + 1..], None => return false }
    }
}

wrap_resource_request_handler! {
    pub struct Filter {}
    impl ResourceRequestHandler {
        fn on_before_resource_load(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, request: Option<&mut Request>, _callback: Option<&mut Callback>) -> ReturnValue {
            let Some(r) = request else { return ReturnValue::CONTINUE };
            let url = userfree(r.url());
            // Never block top-level navigations (the user typed or clicked them).
            let is_main = r.resource_type() == ResourceType::MAIN_FRAME;
            if ENABLED.load(Ordering::Relaxed) && !is_main && is_tracker(&url) {
                BLOCKED.fetch_add(1, Ordering::Relaxed);
                return ReturnValue::CANCEL;
            }
            if DNT.load(Ordering::Relaxed) {
                r.set_header_by_name(Some(&cs("DNT")), Some(&cs("1")), 1);
                r.set_header_by_name(Some(&cs("Sec-GPC")), Some(&cs("1")), 1);
            }
            ReturnValue::CONTINUE
        }
    }
}

pub fn resource_handler() -> Option<ResourceRequestHandler> { Some(Filter::new()) }

// ------------------------------------------------------------------ site permissions

/// Remembered decisions: origin → (permission bit → allow).
static DECISIONS: RwLock<Option<HashMap<String, HashMap<u32, bool>>>> = RwLock::new(None);

thread_local! {
    static PROMPTS: std::cell::RefCell<HashMap<u64, (PermissionPromptCallback, String, u32)>> = Default::default();
    static MEDIA: std::cell::RefCell<HashMap<u64, (MediaAccessCallback, String, u32)>> = Default::default();
}
static NEXT_MEDIA: AtomicU64 = AtomicU64::new(1 << 40);

pub fn load_decisions(path: &std::path::Path) {
    let m: HashMap<String, HashMap<u32, bool>> = emerald_core::store::load_json(path);
    *DECISIONS.write().unwrap() = Some(m);
}

pub fn decisions_json() -> serde_json::Value {
    json!(DECISIONS.read().unwrap().clone().unwrap_or_default())
}

fn decided(origin: &str, perms: u32) -> Option<bool> {
    let g = DECISIONS.read().unwrap();
    let site = g.as_ref()?.get(origin)?;
    // All requested bits must have the same remembered answer.
    let mut ans = None;
    for bit in (0..32).map(|i| 1u32 << i).filter(|b| perms & b != 0) {
        let a = *site.get(&bit)?;
        if ans.is_some_and(|x| x != a) { return None; }
        ans = Some(a);
    }
    ans
}

pub fn set_decision(path: &std::path::Path, origin: &str, perms: u32, allow: Option<bool>) {
    let mut g = DECISIONS.write().unwrap();
    let m = g.get_or_insert_with(HashMap::new);
    let site = m.entry(origin.to_string()).or_default();
    for bit in (0..32).map(|i| 1u32 << i).filter(|b| perms & b != 0) {
        match allow { Some(a) => { site.insert(bit, a); } None => { site.remove(&bit); } }
    }
    m.retain(|_, v| !v.is_empty());
    let _ = emerald_core::store::save_json(path, &*m);
}

/// CEF permission bits → readable names (cef_permission_request_types_t).
pub fn perm_names(perms: u32) -> Vec<&'static str> {
    const NAMES: &[(u32, &str)] = &[
        (1 << 0, "accessibility events"), (1 << 1, "AR"), (1 << 2, "camera pan/tilt/zoom"), (1 << 3, "camera"),
        (1 << 4, "captured surface control"), (1 << 5, "clipboard"), (1 << 6, "top-level storage access"), (1 << 7, "disk quota"),
        (1 << 8, "local fonts"), (1 << 9, "location"), (1 << 10, "identity provider"), (1 << 11, "idle detection"),
        (1 << 12, "microphone"), (1 << 13, "MIDI sysex"), (1 << 14, "multiple downloads"), (1 << 15, "notifications"),
        (1 << 16, "keyboard lock"), (1 << 17, "pointer lock"), (1 << 18, "protected media"), (1 << 19, "registered protocol"),
        (1 << 20, "storage access"), (1 << 21, "VR"), (1 << 22, "window management"), (1 << 23, "file system access"),
        (1 << 24, "local network access"),
    ];
    NAMES.iter().filter(|(b, _)| perms & b != 0).map(|x| x.1).collect()
}

const NOTIFICATIONS: u32 = 1 << 15;

pub fn respond(prompt: u64, allow: bool, remember: bool, store_path: &std::path::Path) {
    if let Some((cb, origin, perms)) = PROMPTS.with(|p| p.borrow_mut().remove(&prompt)) {
        if remember { set_decision(store_path, &origin, perms, Some(allow)); }
        cb.cont(if allow { PermissionRequestResult::ACCEPT } else { PermissionRequestResult::DENY });
    } else if let Some((cb, origin, perms)) = MEDIA.with(|p| p.borrow_mut().remove(&prompt)) {
        if remember { set_decision(store_path, &origin, perms, Some(allow)); }
        if allow { cb.cont(perms) } else { cb.cancel() }
    }
}

wrap_permission_handler! {
    pub struct Permissions {}
    impl PermissionHandler {
        fn on_show_permission_prompt(&self, _browser: Option<&mut Browser>, prompt_id: u64, requesting_origin: Option<&CefString>, requested_permissions: u32, callback: Option<&mut PermissionPromptCallback>) -> c_int {
            let Some(cb) = callback else { return 0 };
            let origin = opt_str(requesting_origin);
            if let Some(allow) = decided(&origin, requested_permissions) {
                cb.cont(if allow { PermissionRequestResult::ACCEPT } else { PermissionRequestResult::DENY });
                return 1;
            }
            // Quiet by default: notification requests are dismissed without a prompt.
            let quiet = shell::query(|s| s.settings.adhd.quiet).unwrap_or(true);
            if requested_permissions == NOTIFICATIONS && quiet {
                cb.cont(PermissionRequestResult::DISMISS);
                return 1;
            }
            PROMPTS.with(|p| p.borrow_mut().insert(prompt_id, (cb.clone(), origin.clone(), requested_permissions)));
            let names = perm_names(requested_permissions);
            shell::with(move |s| s.emit_ui("permission", json!({"id": prompt_id, "origin": origin, "perms": names})));
            1
        }
        fn on_dismiss_permission_prompt(&self, _browser: Option<&mut Browser>, prompt_id: u64, _result: PermissionRequestResult) {
            PROMPTS.with(|p| p.borrow_mut().remove(&prompt_id));
            shell::with(move |s| s.emit_ui("permissionDone", json!({"id": prompt_id})));
        }
        fn on_request_media_access_permission(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, requesting_origin: Option<&CefString>, requested_permissions: u32, callback: Option<&mut MediaAccessCallback>) -> c_int {
            let Some(cb) = callback else { return 0 };
            let origin = opt_str(requesting_origin);
            // Media bits use cef_media_access_permission_types_t (audio=1, video=2, desktop audio=4, desktop video=8).
            let key = requested_permissions << 26; // keep separate from prompt permission bits when remembered
            if let Some(allow) = decided(&origin, key) {
                if allow { cb.cont(requested_permissions) } else { cb.cancel() }
                return 1;
            }
            let id = NEXT_MEDIA.fetch_add(1, Ordering::Relaxed);
            MEDIA.with(|p| p.borrow_mut().insert(id, (cb.clone(), origin.clone(), requested_permissions)));
            let mut names = vec![];
            if requested_permissions & 1 != 0 { names.push("microphone"); }
            if requested_permissions & 2 != 0 { names.push("camera"); }
            if requested_permissions & 12 != 0 { names.push("screen sharing"); }
            shell::with(move |s| s.emit_ui("permission", json!({"id": id, "origin": origin, "perms": names})));
            1
        }
    }
}

pub fn permission_handler() -> PermissionHandler { Permissions::new() }

/// Chromium prefs applied on startup (HTTPS-First, third-party cookies).
pub fn apply_prefs(s: &emerald_core::settings::Settings) -> Vec<String> {
    let mut report = vec![];
    let Some(ctx) = request_context_get_global_context() else { return report };
    let mut set = |name: &str, v: Value| {
        let mut v = v;
        let mut err = CefString::default();
        if ctx.can_set_preference(Some(&cs(name))) != 0 {
            let ok = ctx.set_preference(Some(&cs(name)), Some(&mut v), Some(&mut err)) != 0;
            report.push(format!("{name}={}", if ok { "ok".into() } else { err.to_string() }));
        } else {
            report.push(format!("{name}=unavailable"));
        }
    };
    let b = |x: bool| { let v = value_create().unwrap(); v.set_bool(x as c_int); v };
    let i = |x: i32| { let v = value_create().unwrap(); v.set_int(x); v };
    set("https_only_mode_enabled", b(s.privacy.https_first));
    set("profile.cookie_controls_mode", i(if s.privacy.block_third_party_cookies { 1 } else { 0 }));
    set("enable_do_not_track", b(s.privacy.send_do_not_track));
    // Emerald provides its own password vault; Chrome's manager has no UI here.
    set("credentials_enable_service", b(false));
    set("profile.password_manager_enabled", b(false));
    set("autofill.profile_enabled", b(true));
    let _ = userfree;
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracker_suffix_match() {
        assert!(is_tracker("https://securepubads.g.doubleclick.net/tag/js/gpt.js"));
        assert!(is_tracker("https://www.google-analytics.com/analytics.js"));
        assert!(!is_tracker("https://www.google.com/"));
        assert!(!is_tracker("https://example.com/doubleclick.net"));
    }
}
