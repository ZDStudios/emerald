//! The Emerald window: owns the session, the live tab browsers and the UI views.
//!
//! Everything here runs on the CEF UI thread. CEF callbacks reach the shell through
//! [`with`], which defers the call to the next loop turn if the shell is already borrowed.
//! That keeps re-entrant callbacks (some CEF calls fire delegates synchronously) safe.
use crate::native::{self, Rect};
use crate::util::{self, cs, later};
use cef::*;
use emerald_core::paths::Paths;
use emerald_core::session::{Id, Session};
use emerald_core::settings::Settings;
use emerald_core::theme::Theme;
use emerald_core::{now_ms, store};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;

pub type Handle = cef::sys::cef_window_handle_t;

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
}

/// Run `f` with the shell. If the shell is busy (re-entrant callback), `f` runs on the next
/// turn of the message loop instead.
pub fn with(f: impl FnOnce(&mut Shell) + 'static) {
    let deferred = SHELL.with(|s| match s.try_borrow_mut() {
        Ok(mut g) => { if let Some(sh) = g.as_mut() { f(sh); } None }
        Err(_) => Some(f),
    });
    if let Some(f) = deferred { later(0, move || with(f)); }
}

/// Like [`with`] but returns a value; `None` if the shell is busy or not created.
pub fn query<R>(f: impl FnOnce(&mut Shell) -> R) -> Option<R> {
    SHELL.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

pub fn install(shell: Shell) { SHELL.with(|s| *s.borrow_mut() = Some(shell)); }

#[derive(Default)]
pub struct Live {
    pub browser: Option<Browser>,
    pub handle: Option<Handle>,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub audible: bool,
    pub crashed: bool,
    pub progress: f64,
    pub placed: Option<Option<Rect>>,
    /// Close requested before the browser finished being created.
    pub doomed: bool,
}

pub struct Overlay {
    pub browser: Option<Browser>,
    pub handle: Option<Handle>,
    pub creating: bool,
    pub visible: bool,
    pub rect: Rect,
    pub mode: String,
    pub arg: Value,
}

pub struct Shell {
    pub paths: Paths,
    pub settings: Settings,
    pub session: Session,
    pub themes: Vec<Theme>,
    pub window: Option<Window>,
    pub ui: Option<Browser>,
    pub overlay: Overlay,
    pub live: HashMap<Id, Live>,
    /// Content rectangle in physical pixels, relative to the window client area.
    pub content: Rect,
    pub dpr: f64,
    pub focus_mode: bool,
    pub closing: bool,
    push_scheduled: bool,
    save_scheduled: bool,
    /// Browsers we are about to create, so `on_after_created` can be told apart from
    /// browsers Chrome created on its own (extension tabs) – see `adopt_foreign`.
    pub expected: usize,
    pub ext: crate::extensions::ExtState,
    pub focus: crate::focus::FocusState,
    pub find: Option<(Id, String)>,
    pub downloads: Vec<crate::downloads::Download>,
    pub history: Option<crate::history::History>,
    pub reader_pending: std::collections::HashSet<Id>,
    pub adopted: std::collections::HashSet<i32>,
    pub vault: Option<emerald_core::vault::Vault>,
    pub pw_nonce: HashMap<Id, String>,
    pub pw_pending: HashMap<Id, (String, String, String)>,
    pub pw_never: std::collections::HashSet<String>,
}

impl Shell {
    pub fn new(paths: Paths) -> Self {
        let settings: Settings = store::load_json(&paths.file("settings.json"));
        let mut session: Session = store::load_json(&paths.file("session.json"));
        session.normalize();
        if !settings.startup.restore_session {
            let keep_parked = std::mem::take(&mut session.parked);
            session = Session { parked: keep_parked, ..Session::default() };
        }
        let history = crate::history::History::open(&paths.file("history.db")).map_err(|e| eprintln!("emerald: history disabled: {e}")).ok();
        let mut sh = Self {
            themes: vec![],
            paths, settings, session,
            window: None, ui: None,
            overlay: Overlay { browser: None, handle: None, creating: false, visible: false, rect: Rect::default(), mode: String::new(), arg: Value::Null },
            live: HashMap::new(),
            content: Rect::default(), dpr: 1.0,
            focus_mode: false, closing: false,
            push_scheduled: false, save_scheduled: false,
            expected: 0,
            ext: Default::default(),
            focus: Default::default(),
            find: None,
            downloads: vec![],
            history,
            reader_pending: Default::default(),
            adopted: Default::default(),
            vault: None,
            pw_nonce: HashMap::new(),
            pw_pending: HashMap::new(),
            pw_never: Default::default(),
        };
        sh.reload_themes();
        sh.apply_theme();
        crate::extensions::load_state(&mut sh);
        sh
    }

    // ------------------------------------------------------------------ persistence

    pub fn save_settings(&mut self) {
        if let Err(e) = store::save_json(&self.paths.file("settings.json"), &self.settings) { eprintln!("emerald: saving settings: {e}"); }
    }

    pub fn mark_session_dirty(&mut self) {
        if self.save_scheduled { return; }
        self.save_scheduled = true;
        later(1500, || with(|s| { s.save_scheduled = false; s.save_session_now(); }));
    }

    pub fn save_session_now(&mut self) {
        if let Err(e) = store::save_json(&self.paths.file("session.json"), &self.session) { eprintln!("emerald: saving session: {e}"); }
    }

    // ------------------------------------------------------------------ themes

    pub fn reload_themes(&mut self) {
        let mut themes = emerald_core::theme::builtins();
        if let Ok(rd) = std::fs::read_dir(self.paths.themes()) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "json") {
                    let parsed = std::fs::read_to_string(e.path()).ok().and_then(|s| serde_json::from_str::<Theme>(&s).ok()).and_then(|t| t.validated(false).ok());
                    match parsed {
                        Some(t) if !themes.iter().any(|b| b.id == t.id && b.builtin) => themes.push(t),
                        Some(_) => {}
                        None => eprintln!("emerald: skipping invalid theme {}", e.path().display()),
                    }
                }
            }
        }
        self.themes = themes;
    }

    pub fn current_theme(&self) -> Theme {
        self.themes.iter().find(|t| t.id == self.settings.theme).cloned().unwrap_or_else(|| self.themes[0].clone())
    }

    /// Recompute the stylesheet served at `emerald://*/theme.css` and tell every Emerald
    /// surface to reload it.
    pub fn apply_theme(&mut self) {
        let theme = self.current_theme();
        let fallback = self.themes.iter().find(|t| t.id == "emerald-dark").cloned();
        let mut css = theme.to_css(fallback.as_ref());
        let a = &self.settings.adhd;
        css.push_str(&format!(
            "\n:root{{--em-motion:{};--em-target:{}px;}}\n",
            if a.reduced_motion { "0" } else { "1" },
            if a.big_targets { 44 } else { 32 }
        ));
        if a.dyslexia_font {
            css.push_str("@font-face{font-family:'OpenDyslexic';src:url(fonts/OpenDyslexic-Regular.woff2) format('woff2');}\n:root{--em-font:'OpenDyslexic','Atkinson Hyperlegible',Verdana,sans-serif;}\n");
        }
        if a.reduced_motion {
            css.push_str("*,*::before,*::after{animation-duration:0s!important;transition-duration:0s!important;scroll-behavior:auto!important}\n");
        }
        css.push_str("\n/* user css */\n");
        css.push_str(&self.settings.custom_css);
        crate::scheme::set_theme_css(css);
        self.emit_all("theme", json!({ "id": theme.id, "mode": theme.mode }));
    }

    // ------------------------------------------------------------------ UI messaging

    /// Send an event to the shell UI, the overlay and any open emerald:// tab.
    pub fn emit_all(&self, name: &str, data: Value) {
        let code = format!("window.__em&&__em.emit({},{})", serde_json::to_string(name).unwrap(), data);
        for b in self.emerald_browsers() { util::exec_js(&b, &code); }
    }

    pub fn emit_ui(&self, name: &str, data: Value) {
        let code = format!("window.__em&&__em.emit({},{})", serde_json::to_string(name).unwrap(), data);
        if let Some(b) = &self.ui { util::exec_js(b, &code); }
    }

    pub fn emerald_browsers(&self) -> Vec<Browser> {
        let mut v: Vec<Browser> = self.ui.iter().cloned().collect();
        v.extend(self.overlay.browser.iter().cloned());
        for l in self.live.values() {
            if let Some(b) = &l.browser { if util::browser_url(b).starts_with("emerald://") { v.push(b.clone()); } }
        }
        v
    }

    /// Coalesce state pushes: many changes in one loop turn produce one update.
    pub fn push_state(&mut self) {
        if self.push_scheduled { return; }
        self.push_scheduled = true;
        later(16, || with(|s| { s.push_scheduled = false; let st = s.state_json(); s.emit_all("state", st); }));
    }

    pub fn state_json(&self) -> Value {
        let ss = &self.session;
        let tabs: Vec<Value> = ss.tabs.iter().map(|t| {
            let l = self.live.get(&t.id);
            json!({
                "id": t.id, "workspace": t.workspace, "folder": t.folder, "pinned": t.pinned,
                "url": t.url, "title": t.title, "favicon": t.favicon, "muted": t.muted,
                "live": l.is_some_and(|l| l.browser.is_some() || !l.doomed),
                "loading": l.is_some_and(|l| l.loading), "progress": l.map(|l| l.progress).unwrap_or(0.0),
                "canBack": l.is_some_and(|l| l.can_back), "canForward": l.is_some_and(|l| l.can_forward),
                "audible": l.is_some_and(|l| l.audible), "crashed": l.is_some_and(|l| l.crashed),
                "lastActive": t.last_active,
            })
        }).collect();
        let ws = ss.active_workspace;
        json!({
            "workspaces": ss.workspaces, "activeWorkspace": ws, "tabs": tabs, "folders": ss.folders,
            "rows": ss.rows(ws), "pinned": ss.pinned(ws), "activeTab": ss.active_tab(),
            "split": ss.active().split, "parked": ss.parked, "closedCount": ss.closed.len(),
            "focusMode": self.focus_mode, "settings": self.settings,
            "theme": self.current_theme().id, "focus": self.focus.to_json(&self.settings),
            "extensions": self.ext.pinned_json(),
            "downloads": self.downloads.iter().filter(|d| !d.dismissed).count(),
            "find": self.find.as_ref().map(|f| json!({"tab": f.0, "text": f.1})),
        })
    }

    // ------------------------------------------------------------------ layout

    /// Which tabs are on screen and where.
    pub fn visible(&self) -> Vec<(Id, Rect)> {
        let c = self.content;
        if c.is_empty() { return vec![]; }
        let w = self.session.active();
        let Some(active) = w.active_tab else { return vec![] };
        let is_virtual = |id: Id| self.session.tab(id).is_some_and(|t| is_virtual_url(&t.url));
        if let Some(sp) = &w.split {
            if !self.focus_mode && (sp.left == active || sp.right == active) {
                let gap = (6.0 * self.dpr).round() as i32;
                let lw = ((c.w - gap) as f64 * sp.ratio.clamp(0.2, 0.8)).round() as i32;
                let mut v = vec![];
                if !is_virtual(sp.left) { v.push((sp.left, Rect { x: c.x, y: c.y, w: lw, h: c.h })); }
                if !is_virtual(sp.right) { v.push((sp.right, Rect { x: c.x + lw + gap, y: c.y, w: c.w - lw - gap, h: c.h })); }
                return v;
            }
        }
        if is_virtual(active) { return vec![]; }
        vec![(active, c)]
    }

    pub fn apply_layout(&mut self) {
        let vis = self.visible();
        for (id, r) in &vis {
            if !self.live.contains_key(id) { self.spawn(*id, Some(*r)); }
        }
        for (id, l) in self.live.iter_mut() {
            let want = vis.iter().find(|(v, _)| v == id).map(|x| x.1);
            if l.placed != Some(want) {
                if let Some(h) = l.handle { native::place(h, want); l.placed = Some(want); }
            }
        }
        if self.overlay.visible {
            if let Some(h) = self.overlay.handle { native::place(h, Some(self.overlay.rect)); native::raise(h); }
        }
    }

    pub fn set_content_rect(&mut self, css: Rect, dpr: f64) {
        self.dpr = if dpr > 0.0 { dpr } else { 1.0 };
        let s = |v: i32| (v as f64 * self.dpr).round() as i32;
        self.content = Rect { x: s(css.x), y: s(css.y), w: s(css.w), h: s(css.h) };
        self.apply_layout();
        if self.overlay.visible { self.position_overlay(); }
    }

    // ------------------------------------------------------------------ tab browsers

    pub fn parent_handle(&self) -> Option<Handle> {
        self.window.as_ref().map(|w| w.window_handle()).filter(|h| !native::is_null(*h))
    }

    /// Create the CEF browser for a tab (waking it if it was asleep).
    pub fn spawn(&mut self, id: Id, rect: Option<Rect>) {
        if self.live.contains_key(&id) || self.closing { return; }
        let Some(tab) = self.session.tab(id) else { return };
        if is_virtual_url(&tab.url) { return; }
        let Some(parent) = self.parent_handle() else { return };
        let url = if tab.url.is_empty() { "about:blank".to_string() } else { tab.url.clone() };
        let bounds = rect.unwrap_or(Rect { x: 0, y: 0, w: 1, h: 1 });
        #[allow(unused_mut)]
        let mut wi = WindowInfo { runtime_style: RuntimeStyle::CHROME, ..Default::default() }.set_as_child(parent, &bounds.to_cef());
        #[cfg(target_os = "windows")]
        if rect.is_none() {
            wi.style &= !windows_sys::Win32::UI::WindowsAndMessaging::WS_VISIBLE;
        }
        let mut client = crate::client::tab_client(id);
        let bs = BrowserSettings { background_color: self.bg_color(), ..Default::default() };
        self.live.insert(id, Live { placed: Some(rect), ..Default::default() });
        self.expected += 1;
        browser_host_create_browser(Some(&wi), Some(&mut client), Some(&cs(&url)), Some(&bs), None, None);
        self.push_state();
    }

    fn bg_color(&self) -> u32 {
        let t = self.current_theme();
        let hex = t.colors.get("bg").cloned().unwrap_or_default();
        let h = hex.trim_start_matches('#');
        if h.len() == 6 {
            if let Ok(v) = u32::from_str_radix(h, 16) { return 0xff00_0000 | v; }
        }
        0xff0e_1512
    }

    pub fn on_tab_created(&mut self, id: Id, browser: Browser) {
        self.expected = self.expected.saturating_sub(1);
        let handle = browser.host().map(|h| h.window_handle());
        let Some(l) = self.live.get_mut(&id) else {
            // Created for a tab that was closed meanwhile.
            if let Some(h) = browser.host() { h.close_browser(1); }
            return;
        };
        let _ = &mut self.expected;
        if l.doomed {
            if let Some(h) = browser.host() { h.close_browser(1); }
            return;
        }
        l.browser = Some(browser.clone());
        l.handle = handle;
        l.placed = None; // force placement
        if let Some(t) = self.session.tab(id) {
            if let Some(h) = browser.host() {
                if t.zoom != 0.0 { h.set_zoom_level(t.zoom); }
                if t.muted { h.set_audio_muted(1); }
            }
        }
        self.apply_layout();
        if self.session.active_tab() == Some(id) { self.focus_tab(id); }
        self.push_state();
    }

    /// A popup or Chrome-created browser that arrived already attached to a tab id.
    pub fn adopt_created(&mut self, id: Id, browser: Browser) {
        self.live.entry(id).or_default();
        self.expected += 1; // balanced by on_tab_created
        self.on_tab_created(id, browser);
    }

    pub fn on_tab_closed(&mut self, id: Id, browser_id: i32) {
        if let Some(l) = self.live.get(&id) {
            if l.browser.as_ref().is_some_and(|b| b.identifier() == browser_id) || l.doomed {
                self.live.remove(&id);
            }
        }
        if self.closing && self.live.is_empty() { self.finish_close(); }
        self.push_state();
    }

    fn kill_browser(&mut self, id: Id) {
        if let Some(mut l) = self.live.remove(&id) {
            match l.browser.take() {
                Some(b) => {
                    if let Some(h) = l.handle { native::place(h, None); }
                    if let Some(h) = b.host() { h.close_browser(1); }
                    // Keep a doomed entry until on_before_close so window-close waits for it.
                    if self.closing { self.live.insert(id, Live { doomed: true, browser: Some(b), ..Default::default() }); }
                }
                None => { self.live.insert(id, Live { doomed: true, ..Default::default() }); }
            }
        }
    }

    pub fn focus_tab(&self, id: Id) {
        if let Some(h) = self.live.get(&id).and_then(|l| l.browser.as_ref()).and_then(|b| b.host()) { h.set_focus(1); }
    }

    pub fn browser(&self, id: Id) -> Option<Browser> { self.live.get(&id).and_then(|l| l.browser.clone()) }

    pub fn active_browser(&self) -> Option<Browser> { self.session.active_tab().and_then(|id| self.browser(id)) }

    pub fn tab_for_browser(&self, browser_id: i32) -> Option<Id> {
        self.live.iter().find(|(_, l)| l.browser.as_ref().is_some_and(|b| b.identifier() == browser_id)).map(|(id, _)| *id)
    }

    // ------------------------------------------------------------------ tab operations

    pub fn new_tab(&mut self, url: Option<&str>, opener: Option<Id>, background: bool) -> Id {
        let url = url.map(String::from).unwrap_or_else(|| "emerald://start/".into());
        let id = self.session.new_tab(&url, opener, !background);
        // Tabs opened in the background start loading right away (hidden), as people expect;
        // only restored tabs are lazy. Idle ones go back to sleep after the timeout.
        if background { self.spawn(id, None); }
        self.after_tab_change();
        self.nudge_tab_limit();
        id
    }

    pub fn activate(&mut self, id: Id) {
        if self.session.tab(id).is_none() { return; }
        self.session.activate(id);
        self.after_tab_change();
        self.focus_tab(id);
    }

    pub fn close_tab(&mut self, id: Id) {
        let next = self.session.close_tab(id);
        self.kill_browser(id);
        if let Some(n) = next { self.session.activate(n); }
        self.after_tab_change();
        if let Some(n) = next { self.focus_tab(n); }
    }

    /// Put a tab to sleep: its browser is closed; metadata stays.
    pub fn sleep_tab(&mut self, id: Id) {
        if self.visible().iter().any(|(v, _)| *v == id) { return; }
        if self.live.get(&id).is_some_and(|l| l.browser.is_some()) {
            self.kill_browser(id);
            self.push_state();
        }
    }

    pub fn navigate(&mut self, id: Id, url: &str) {
        if let Some(t) = self.session.tab_mut(id) { t.url = url.to_string(); }
        if is_virtual_url(url) {
            self.kill_browser(id);
        } else if let Some(b) = self.browser(id) {
            if let Some(f) = b.main_frame() { f.load_url(Some(&cs(url))); }
        }
        self.after_tab_change();
    }

    pub fn after_tab_change(&mut self) {
        self.apply_layout();
        self.push_state();
        self.mark_session_dirty();
    }

    /// Sleep background tabs idle longer than the configured timeout.
    pub fn sleep_sweep(&mut self) {
        let m = &self.settings.memory;
        if m.sleep_after_minutes == 0 { return; }
        let cutoff = now_ms().saturating_sub(m.sleep_after_minutes as u64 * 60_000);
        let vis: Vec<Id> = self.visible().iter().map(|v| v.0).collect();
        let candidates: Vec<Id> = self.live.iter().filter(|(id, l)| {
            let Some(t) = self.session.tab(**id) else { return false };
            l.browser.is_some() && !vis.contains(id) && t.last_active < cutoff
                && !(m.never_sleep_pinned && t.pinned) && !(m.never_sleep_audible && l.audible)
        }).map(|(id, _)| *id).collect();
        for id in candidates { self.sleep_tab(id); }
    }

    /// Put every tab that isn't on screen to sleep ("Free memory").
    pub fn sleep_all_background(&mut self) -> usize {
        let vis: Vec<Id> = self.visible().iter().map(|v| v.0).collect();
        let ids: Vec<Id> = self.live.iter().filter(|(id, l)| l.browser.is_some() && !vis.contains(id) && !l.audible).map(|(id, _)| *id).collect();
        for id in &ids { self.sleep_tab(*id); }
        ids.len()
    }

    fn nudge_tab_limit(&mut self) {
        let lim = self.settings.adhd.tab_limit;
        if lim == 0 { return; }
        let n = self.session.tabs_in(self.session.active_workspace).count() as u32;
        if n > lim {
            self.emit_ui("nudge", json!({ "kind": "tabs", "count": n, "limit": lim,
                "text": format!("{n} tabs open in this space. Park a few for later?") }));
        }
    }

    // ------------------------------------------------------------------ overlay

    pub fn show_overlay(&mut self, mode: &str, arg: Value) {
        let Some(parent) = self.parent_handle() else { return };
        self.overlay.visible = true;
        self.overlay.mode = mode.to_string();
        self.overlay.arg = arg.clone();
        self.position_overlay();
        if self.overlay.browser.is_none() {
            if !self.overlay.creating {
                let r = self.overlay.rect;
                let wi = WindowInfo { runtime_style: RuntimeStyle::ALLOY, ..Default::default() }.set_as_child(parent, &r.to_cef());
                let mut client = crate::client::overlay_client();
                let bs = BrowserSettings { background_color: self.bg_color(), ..Default::default() };
                browser_host_create_browser(Some(&wi), Some(&mut client), Some(&cs("emerald://overlay/")), Some(&bs), None, None);
                self.overlay.creating = true;
            }
            self.ext.pending_overlay = Some((mode.to_string(), arg));
            return;
        }
        if let Some(h) = self.overlay.handle { native::place(h, Some(self.overlay.rect)); native::raise(h); }
        if let Some(b) = &self.overlay.browser {
            util::exec_js(b, &format!("window.__em&&__em.emit('open',{})", json!({"mode": mode, "arg": arg})));
            if let Some(h) = b.host() { h.set_focus(1); }
        }
    }

    pub fn on_overlay_created(&mut self, b: Browser) {
        self.overlay.creating = false;
        self.overlay.handle = b.host().map(|h| h.window_handle());
        self.overlay.browser = Some(b.clone());
        if self.overlay.visible {
            if let Some(h) = self.overlay.handle { native::place(h, Some(self.overlay.rect)); native::raise(h); }
        } else if let Some(h) = self.overlay.handle { native::place(h, None); }
    }

    /// Called by the overlay page once its script is ready.
    pub fn overlay_ready(&mut self) {
        if let Some((mode, arg)) = self.ext.pending_overlay.take() { self.show_overlay(&mode, arg); }
    }

    pub fn hide_overlay(&mut self) {
        self.overlay.visible = false;
        if let Some(h) = self.overlay.handle { native::place(h, None); }
        if let Some(id) = self.session.active_tab() { self.focus_tab(id); }
    }

    fn position_overlay(&mut self) {
        let d = self.dpr;
        let px = |v: f64| (v * d).round() as i32;
        let c = if self.content.is_empty() { Rect { x: 0, y: 0, w: px(900.0), h: px(700.0) } } else { self.content };
        let (w, h) = match self.overlay.mode.as_str() {
            "menu" => {
                let items = self.overlay.arg["items"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
                let row = if self.settings.adhd.big_targets { 44.0 } else { 32.0 };
                let hgt: f64 = items.iter().map(|i| if i["sep"].as_bool() == Some(true) { 11.0 } else if i.get("label").is_some() { 28.0 } else { row }).sum::<f64>() + 12.0;
                (px(260.0), px(hgt).min(c.h + c.y - px(8.0)))
            }
            "park" => (px(460.0), px(360.0)),
            "note" => (px(520.0), px(300.0)),
            "memory" => (px(580.0), px(500.0)),
            _ => (px(640.0), px(470.0)),
        };
        let w = w.min(c.w - px(16.0)).max(px(160.0));
        let h = h.min(c.h - px(16.0)).max(px(100.0));
        self.overlay.rect = if self.overlay.mode == "menu" {
            // Anchor at the click point (CSS px), kept inside the window's content area.
            let ax = px(self.overlay.arg["x"].as_f64().unwrap_or(0.0));
            let ay = px(self.overlay.arg["y"].as_f64().unwrap_or(0.0));
            let x = (ax - w).max(c.x + px(4.0)).min(c.x + c.w - w - px(4.0));
            let x = if ax < c.x + c.w / 2 { ax.max(c.x + px(4.0)).min(c.x + c.w - w - px(4.0)) } else { x };
            Rect { x, y: ay.max(c.y).min(c.y + c.h - h - px(4.0)), w, h }
        } else {
            Rect { x: c.x + (c.w - w) / 2, y: c.y + px(48.0).min((c.h - h) / 2).max(0), w, h }
        };
        if self.overlay.visible { if let Some(h) = self.overlay.handle { native::place(h, Some(self.overlay.rect)); native::raise(h); } }
    }

    // ------------------------------------------------------------------ window

    pub fn request_close(&mut self) -> bool {
        if self.closing && self.live.is_empty() { return true; }
        if !self.closing {
            self.closing = true;
            self.save_session_now();
            let ids: Vec<Id> = self.live.keys().copied().collect();
            for id in ids { self.kill_browser(id); }
            if let Some(b) = self.overlay.browser.take() { if let Some(h) = b.host() { h.close_browser(1); } }
            self.ext.shutdown();
            if self.live.is_empty() { return true; }
            // Safety net: never hang on exit if a browser fails to report closing.
            later(3000, || with(|s| { s.live.clear(); s.finish_close(); }));
        }
        false
    }

    fn finish_close(&mut self) {
        if let Some(w) = self.window.clone() { later(0, move || w.close()); }
    }
}

/// `emerald://start` is drawn by the shell UI itself (no browser, no renderer).
pub fn is_virtual_url(url: &str) -> bool {
    url.is_empty() || url.starts_with("emerald://start")
}
