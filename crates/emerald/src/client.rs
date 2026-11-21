//! CEF client handlers.
//!
//! * `TabClient` – one per tab browser (Chrome style, native child).
//! * `UiClient` – the shell UI BrowserView (Alloy).
//! * `OverlayClient` – the command bar / panel overlay (Alloy, native child).
//! * `ForeignClient` – browsers Chrome creates on its own (e.g. `chrome.tabs.create` from an
//!   extension). They are adopted into Emerald tabs and closed.
use crate::native::Rect;
use crate::shell::{self, is_virtual_url};
use crate::util::{self, cs, opt_str};
use cef::*;
use emerald_core::session::Id;
use std::os::raw::c_int;

/// Which surface an event came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source { Ui, Overlay, Tab(Id), Foreign }

const IPC_PREFIX: &str = "\u{1}EMERALD:";

/// UI → Rust messages travel as console messages from emerald:// pages (see ARCHITECTURE.md §3).
fn handle_console(src: Source, browser: Option<&mut Browser>, message: Option<&CefString>, source: Option<&CefString>) -> c_int {
    let msg = opt_str(message);
    let Some(payload) = msg.strip_prefix(IPC_PREFIX) else { return 0 };
    let script = opt_str(source);
    let Some(b) = browser.cloned() else { return 1 };
    // Only Emerald's own pages may talk to the shell.
    if !script.starts_with("emerald://") || !util::browser_url(&b).starts_with("emerald://") {
        eprintln!("emerald: rejected IPC from {script}");
        return 1;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else { return 1 };
    let id = v["id"].as_u64().unwrap_or(0);
    let cmd = v["cmd"].as_str().unwrap_or("").to_string();
    let args = v["args"].clone();
    shell::with(move |s| {
        let (ok, data) = match crate::api::handle(s, src, &b, &cmd, &args) {
            Ok(d) => (true, d),
            Err(e) => (false, serde_json::Value::String(e.to_string())),
        };
        if id != 0 {
            util::exec_js(&b, &format!("window.__em&&__em.resolve({id},{ok},{data})"));
        }
    });
    1
}

fn on_key(src: Source, event: Option<&KeyEvent>, is_shortcut: Option<&mut c_int>) -> c_int {
    let Some(e) = event else { return 0 };
    if crate::keys::handle(src, e) {
        if let Some(s) = is_shortcut { *s = 0; }
        return 1;
    }
    0
}

// ====================================================================== tab client

wrap_life_span_handler! {
    pub struct TabLife { tab: Id }
    impl LifeSpanHandler {
        fn on_before_popup(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _popup_id: c_int, target_url: Option<&CefString>, _target_frame_name: Option<&CefString>, target_disposition: WindowOpenDisposition, _user_gesture: c_int, _popup_features: Option<&PopupFeatures>, window_info: Option<&mut WindowInfo>, client: Option<&mut Option<Client>>, _settings: Option<&mut BrowserSettings>, _extra_info: Option<&mut Option<DictionaryValue>>, _no_javascript_access: Option<&mut c_int>) -> c_int {
            let url = opt_str(target_url);
            let disp = sys::cef_window_open_disposition_t::from(target_disposition);
            // Real popup windows (OAuth, payment) keep CEF's own small popup window so
            // `window.opener` and window sizing behave as sites expect.
            if disp == sys::cef_window_open_disposition_t::CEF_WOD_NEW_POPUP {
                if let Some(c) = client { *c = Some(popup_client()); }
                return 0;
            }
            let background = disp == sys::cef_window_open_disposition_t::CEF_WOD_NEW_BACKGROUND_TAB;
            let opener = self.tab;
            // Turn the popup into a native-child tab, keeping the opener relationship.
            let made = shell::query(|s| {
                let parent = s.parent_handle()?;
                let id = s.session.new_tab(&url, Some(opener), !background);
                let rect = if background { None } else { Some(s.content) };
                s.live.insert(id, shell::Live { placed: Some(rect), ..Default::default() });
                s.after_tab_change();
                Some((id, parent, rect))
            }).flatten();
            let Some((id, parent, rect)) = made else {
                let u = url.clone();
                shell::with(move |s| { s.new_tab(Some(&u), Some(opener), background); });
                return 1;
            };
            if let Some(wi) = window_info {
                #[allow(unused_mut)]
                let mut w = WindowInfo { runtime_style: RuntimeStyle::CHROME, ..Default::default() }
                    .set_as_child(parent, &rect.unwrap_or(Rect { x: 0, y: 0, w: 1, h: 1 }).to_cef());
                #[cfg(target_os = "windows")]
                if rect.is_none() { w.style &= !windows_sys::Win32::UI::WindowsAndMessaging::WS_VISIBLE; }
                *wi = w;
            }
            if let Some(c) = client { *c = Some(tab_client(id)); }
            0
        }
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser.cloned() else { return };
            let id = self.tab;
            shell::with(move |s| {
                if s.live.contains_key(&id) { s.on_tab_created(id, b) }
                else if s.session.tab(id).is_some() { s.adopt_created(id, b) }
                else if let Some(h) = b.host() { h.close_browser(1) }
            });
        }
        fn do_close(&self, _browser: Option<&mut Browser>) -> c_int { 0 }
        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let bid = browser.map(|b| b.identifier()).unwrap_or(-1);
            let id = self.tab;
            shell::with(move |s| s.on_tab_closed(id, bid));
        }
    }
}

wrap_display_handler! {
    pub struct TabDisplay { tab: Id }
    impl DisplayHandler {
        fn on_address_change(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.is_none_or(|f| f.is_main() == 0) { return; }
            let url = opt_str(url);
            let id = self.tab;
            shell::with(move |s| {
                if let Some(t) = s.session.tab_mut(id) {
                    if t.url != url { t.url = url.clone(); t.favicon = None; }
                }
                s.on_navigated(id, &url);
                s.push_state();
                s.mark_session_dirty();
            });
        }
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = opt_str(title);
            let id = self.tab;
            shell::with(move |s| {
                if let Some(t) = s.session.tab_mut(id) { t.title = title.clone(); }
                if let (Some(h), Some(t)) = (s.history.as_mut(), s.session.tab(id)) { h.set_title(&t.url, &title); }
                s.push_state();
                s.mark_session_dirty();
            });
        }
        fn on_favicon_urlchange(&self, _browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let first = icon_urls.and_then(|l| l.clone().into_iter().next());
            let id = self.tab;
            shell::with(move |s| {
                if let Some(t) = s.session.tab_mut(id) { t.favicon = first; }
                s.push_state();
                s.mark_session_dirty();
            });
        }
        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            let id = self.tab;
            shell::with(move |s| { if let Some(l) = s.live.get_mut(&id) { l.progress = progress; } s.push_state(); });
        }
        fn on_fullscreen_mode_change(&self, _browser: Option<&mut Browser>, fullscreen: c_int) {
            let on = fullscreen != 0;
            shell::with(move |s| s.emit_ui("fullscreen", serde_json::json!(on)));
        }
        fn on_console_message(&self, browser: Option<&mut Browser>, _level: LogSeverity, message: Option<&CefString>, source: Option<&CefString>, _line: c_int) -> c_int {
            handle_console(Source::Tab(self.tab), browser, message, source)
        }
    }
}

wrap_load_handler! {
    pub struct TabLoad { tab: Id }
    impl LoadHandler {
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: c_int, can_go_back: c_int, can_go_forward: c_int) {
            let id = self.tab;
            shell::with(move |s| {
                if let Some(l) = s.live.get_mut(&id) {
                    l.loading = is_loading != 0; l.can_back = can_go_back != 0; l.can_forward = can_go_forward != 0;
                    if is_loading != 0 { l.crashed = false; }
                }
                s.push_state();
            });
        }
        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: c_int) {
            if frame.is_none_or(|f| f.is_main() == 0) { return; }
            let Some(b) = browser.cloned() else { return };
            let id = self.tab;
            shell::with(move |s| s.on_load_end(id, &b));
        }
    }
}

wrap_keyboard_handler! {
    pub struct TabKeys { tab: Id }
    impl KeyboardHandler {
        fn on_pre_key_event(&self, _browser: Option<&mut Browser>, event: Option<&KeyEvent>, _os_event: Option<&mut sys::XEvent>, is_keyboard_shortcut: Option<&mut c_int>) -> c_int {
            on_key(Source::Tab(self.tab), event, is_keyboard_shortcut)
        }
    }
}

wrap_request_handler! {
    pub struct TabRequests { tab: Id }
    impl RequestHandler {
        fn on_before_browse(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, _user_gesture: c_int, _is_redirect: c_int) -> c_int {
            if frame.is_none_or(|f| f.is_main() == 0) { return 0; }
            let url = request.map(|r| util::userfree(r.url())).unwrap_or_default();
            let id = self.tab;
            // Focus-time site blocker: swap the navigation for the calm "blocked" page.
            if let Some(Some(block_url)) = shell::query(|s| s.focus.block_redirect(&s.settings, &url)) {
                shell::with(move |s| s.navigate(id, &block_url));
                return 1;
            }
            if is_virtual_url(&url) {
                shell::with(move |s| s.navigate(id, "emerald://start/"));
                return 1;
            }
            0
        }
        fn on_open_urlfrom_tab(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, target_url: Option<&CefString>, target_disposition: WindowOpenDisposition, _user_gesture: c_int) -> c_int {
            // Ctrl/middle-click: open in a new Emerald tab instead of a Chrome window.
            let url = opt_str(target_url);
            let bg = sys::cef_window_open_disposition_t::from(target_disposition) != sys::cef_window_open_disposition_t::CEF_WOD_NEW_FOREGROUND_TAB;
            let id = self.tab;
            shell::with(move |s| { s.new_tab(Some(&url), Some(id), bg); });
            1
        }
        fn resource_request_handler(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _request: Option<&mut Request>, _is_navigation: c_int, _is_download: c_int, _request_initiator: Option<&CefString>, _disable_default_handling: Option<&mut c_int>) -> Option<ResourceRequestHandler> {
            crate::privacy::resource_handler()
        }
        fn on_render_process_terminated(&self, _browser: Option<&mut Browser>, _status: TerminationStatus, _error_code: c_int, _error_string: Option<&CefString>) {
            let id = self.tab;
            shell::with(move |s| { if let Some(l) = s.live.get_mut(&id) { l.crashed = true; l.loading = false; } s.push_state(); });
        }
    }
}

wrap_find_handler! {
    pub struct TabFind { tab: Id }
    impl FindHandler {
        fn on_find_result(&self, _browser: Option<&mut Browser>, _identifier: c_int, count: c_int, _selection_rect: Option<&cef::Rect>, active_match_ordinal: c_int, final_update: c_int) {
            shell::with(move |s| s.emit_ui("findResult", serde_json::json!({"count": count, "active": active_match_ordinal, "final": final_update != 0})));
        }
    }
}

wrap_audio_handler! {
    pub struct TabAudio { tab: Id }
    impl AudioHandler {
        fn on_audio_stream_started(&self, _browser: Option<&mut Browser>, _params: Option<&AudioParameters>, _channels: c_int) {
            let id = self.tab;
            shell::with(move |s| { if let Some(l) = s.live.get_mut(&id) { l.audible = true; } s.push_state(); });
        }
        fn on_audio_stream_stopped(&self, _browser: Option<&mut Browser>) {
            let id = self.tab;
            shell::with(move |s| { if let Some(l) = s.live.get_mut(&id) { l.audible = false; } s.push_state(); });
        }
        fn on_audio_stream_error(&self, _browser: Option<&mut Browser>, _message: Option<&CefString>) {}
    }
}

wrap_command_handler! {
    pub struct ChromeCommands { src: Source }
    impl CommandHandler {
        fn on_chrome_command(&self, _browser: Option<&mut Browser>, command_id: c_int, _disposition: WindowOpenDisposition) -> c_int {
            crate::keys::chrome_command(self.src, command_id) as c_int
        }
    }
}

wrap_focus_handler! {
    pub struct HideOverlayOnFocus { src: Source }
    impl FocusHandler {
        fn on_got_focus(&self, _browser: Option<&mut Browser>) {
            // Clicking a page or the shell dismisses the command bar / menus.
            shell::with(|s| if s.overlay.visible { s.overlay.visible = false; if let Some(h) = s.overlay.handle { crate::native::place(h, None); } });
        }
    }
}

wrap_client! {
    pub struct TabClient { tab: Id }
    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(TabLife::new(self.tab)) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(TabDisplay::new(self.tab)) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(TabLoad::new(self.tab)) }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> { Some(TabKeys::new(self.tab)) }
        fn request_handler(&self) -> Option<RequestHandler> { Some(TabRequests::new(self.tab)) }
        fn find_handler(&self) -> Option<FindHandler> { Some(TabFind::new(self.tab)) }
        fn audio_handler(&self) -> Option<AudioHandler> { Some(TabAudio::new(self.tab)) }
        fn download_handler(&self) -> Option<DownloadHandler> { Some(crate::downloads::handler()) }
        fn command_handler(&self) -> Option<CommandHandler> { Some(ChromeCommands::new(Source::Tab(self.tab))) }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> { Some(crate::menus::handler(self.tab)) }
        fn permission_handler(&self) -> Option<PermissionHandler> { Some(crate::privacy::permission_handler()) }
        fn focus_handler(&self) -> Option<FocusHandler> { Some(HideOverlayOnFocus::new(Source::Tab(self.tab))) }
    }
}

pub fn tab_client(id: Id) -> Client { TabClient::new(id) }

// ====================================================================== popup windows

wrap_keyboard_handler! {
    pub struct PopupKeys {}
    impl KeyboardHandler {}
}

wrap_client! {
    pub struct PopupClient {}
    impl Client {
        fn download_handler(&self) -> Option<DownloadHandler> { Some(crate::downloads::handler()) }
    }
}

pub fn popup_client() -> Client { PopupClient::new() }

// ====================================================================== shell UI

wrap_life_span_handler! {
    pub struct UiLife { overlay: bool }
    impl LifeSpanHandler {
        fn on_before_popup(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _popup_id: c_int, target_url: Option<&CefString>, _target_frame_name: Option<&CefString>, _target_disposition: WindowOpenDisposition, _user_gesture: c_int, _popup_features: Option<&PopupFeatures>, _window_info: Option<&mut WindowInfo>, _client: Option<&mut Option<Client>>, _settings: Option<&mut BrowserSettings>, _extra_info: Option<&mut Option<DictionaryValue>>, _no_javascript_access: Option<&mut c_int>) -> c_int {
            let url = opt_str(target_url);
            shell::with(move |s| { s.new_tab(Some(&url), None, false); });
            1
        }
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser.cloned() else { return };
            let overlay = self.overlay;
            shell::with(move |s| if overlay { s.on_overlay_created(b) } else { s.ui = Some(b) });
        }
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            let overlay = self.overlay;
            shell::with(move |s| if overlay { s.overlay.browser = None; s.overlay.handle = None; s.overlay.creating = false; } else { s.ui = None; });
        }
    }
}

wrap_display_handler! {
    pub struct UiDisplay { src: Source }
    impl DisplayHandler {
        fn on_console_message(&self, browser: Option<&mut Browser>, _level: LogSeverity, message: Option<&CefString>, source: Option<&CefString>, _line: c_int) -> c_int {
            handle_console(self.src, browser, message, source)
        }
    }
}

wrap_keyboard_handler! {
    pub struct UiKeys { src: Source }
    impl KeyboardHandler {
        fn on_pre_key_event(&self, _browser: Option<&mut Browser>, event: Option<&KeyEvent>, _os_event: Option<&mut sys::XEvent>, is_keyboard_shortcut: Option<&mut c_int>) -> c_int {
            on_key(self.src, event, is_keyboard_shortcut)
        }
    }
}

wrap_drag_handler! {
    pub struct UiDrag {}
    impl DragHandler {
        fn on_draggable_regions_changed(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, regions: Option<&[DraggableRegion]>) {
            let regions: Vec<DraggableRegion> = regions.map(|r| r.to_vec()).unwrap_or_default();
            shell::with(move |s| if let Some(w) = &s.window { w.set_draggable_regions(Some(&regions)); });
        }
    }
}

wrap_request_handler! {
    pub struct UiRequests {}
    impl RequestHandler {
        fn on_before_browse(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, _user_gesture: c_int, _is_redirect: c_int) -> c_int {
            // The shell UI never navigates away from emerald://; links open in tabs instead.
            if frame.is_none_or(|f| f.is_main() == 0) { return 0; }
            let url = request.map(|r| util::userfree(r.url())).unwrap_or_default();
            if url.starts_with("emerald://") { return 0; }
            shell::with(move |s| { s.new_tab(Some(&url), None, false); });
            1
        }
    }
}

wrap_client! {
    pub struct UiClient { src: Source }
    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(UiLife::new(self.src == Source::Overlay)) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(UiDisplay::new(self.src)) }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> { Some(UiKeys::new(self.src)) }
        fn drag_handler(&self) -> Option<DragHandler> { Some(UiDrag::new()) }
        fn request_handler(&self) -> Option<RequestHandler> { Some(UiRequests::new()) }
        fn command_handler(&self) -> Option<CommandHandler> { Some(ChromeCommands::new(self.src)) }
        fn focus_handler(&self) -> Option<FocusHandler> { if self.src == Source::Ui { Some(HideOverlayOnFocus::new(self.src)) } else { None } }
    }
}

pub fn ui_client() -> Client { UiClient::new(Source::Ui) }
pub fn overlay_client() -> Client { UiClient::new(Source::Overlay) }

// ====================================================================== foreign browsers

wrap_display_handler! {
    pub struct ForeignDisplay {}
    impl DisplayHandler {
        fn on_address_change(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.is_none_or(|f| f.is_main() == 0) { return; }
            let url = opt_str(url);
            if url.is_empty() || url == "about:blank" { return; }
            let Some(b) = browser.cloned() else { return };
            shell::with(move |s| s.adopt_foreign(&b, &url));
        }
    }
}

wrap_life_span_handler! {
    pub struct ForeignLife {}
    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser.cloned() else { return };
            // Some foreign browsers already have their URL (no address change follows).
            let url = util::browser_url(&b);
            if !url.is_empty() && url != "about:blank" { shell::with(move |s| s.adopt_foreign(&b, &url)); }
        }
    }
}

wrap_client! {
    pub struct ForeignClient {}
    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> { Some(ForeignDisplay::new()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(ForeignLife::new()) }
    }
}

pub fn foreign_client() -> Client { ForeignClient::new() }

pub fn ui_browser_view(url: &str) -> Option<BrowserView> {
    let mut client = ui_client();
    let mut delegate = crate::window::UiViewDelegate::new();
    let bs = BrowserSettings { background_color: 0xff0e1512, ..Default::default() };
    browser_view_create(Some(&mut client), Some(&cs(url)), Some(&bs), None, None, Some(&mut delegate))
}
