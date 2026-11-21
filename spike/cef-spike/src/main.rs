//! Phase 0 engine spike: the smallest useful Rust + CEF browser.
//!
//! * Chrome runtime style, hosted in a CEF Views window.
//! * `--toolbar=normal|none` picks whether Chrome's own toolbar is shown, so we can
//!   test whether extensions and the Web Store work with and without it.
//! * `--url=<u>` (repeatable via `--urls=a,b,c`) opens one tab per URL.
//! * Remote debugging on 9222 so the test harness (crates/emerald-probe) can drive it.
//! * `--profile=<dir>` isolates the user data directory.
use cef::*;
use std::cell::RefCell;

fn switch(cmd: &CommandLine, name: &str) -> String {
    CefString::from(&cmd.switch_value(Some(&CefString::from(name)))).to_string()
}

wrap_window_delegate! {
    struct SpikeWindow {
        view: RefCell<Option<BrowserView>>,
        extra: RefCell<Vec<BrowserView>>,
    }
    impl ViewDelegate {
        fn preferred_size(&self, _v: Option<&mut View>) -> Size { Size { width: 1280, height: 800 } }
    }
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let v = self.view.borrow();
            if let (Some(w), Some(bv)) = (window, v.as_ref()) {
                let layout = w.set_to_box_layout(Some(&BoxLayoutSettings { horizontal: 1, cross_axis_alignment: AxisAlignment::STRETCH, ..Default::default() })).unwrap();
                let chrome_first = MODE.with(|m| m.borrow().as_str() == "mixed2");
                if chrome_first {
                    let mut view = View::from(bv);
                    w.add_child_view(Some(&mut view));
                    layout.set_flex_for_view(Some(&mut view), 1);
                }
                for (i, b) in self.extra.borrow().iter().enumerate() {
                    let mut ev = View::from(b);
                    w.add_child_view(Some(&mut ev));
                    if i == 0 && MODE.with(|m| m.borrow().as_str() == "mixed") { ev.set_size(Some(&Size { width: 260, height: 800 })); }
                    else { layout.set_flex_for_view(Some(&mut ev), 1); }
                }
                if !chrome_first {
                    let mut view = View::from(bv);
                    w.add_child_view(Some(&mut view));
                    layout.set_flex_for_view(Some(&mut view), 1);
                }
                WIN.with(|x| *x.borrow_mut() = Some(w.clone()));
                w.show();
            }
        }
        fn on_window_destroyed(&self, _w: Option<&mut Window>) { *self.view.borrow_mut() = None; }
        fn can_close(&self, _w: Option<&mut Window>) -> i32 {
            match self.view.borrow().as_ref().and_then(|v| v.browser()) {
                Some(b) => b.host().map(|h| h.try_close_browser()).unwrap_or(1),
                None => 1,
            }
        }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::CHROME }
    }
}

wrap_browser_view_delegate! {
    struct SpikeView { toolbar: ChromeToolbarType, style: RuntimeStyle }
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn chrome_toolbar_type(&self, _bv: Option<&mut BrowserView>) -> ChromeToolbarType { self.toolbar }
        fn browser_runtime_style(&self) -> RuntimeStyle { self.style }
        fn on_popup_browser_view_created(&self, _bv: Option<&mut BrowserView>, popup: Option<&mut BrowserView>, _dt: i32) -> i32 {
            let mut w = SpikeWindow::new(RefCell::new(popup.cloned()), RefCell::new(vec![]));
            window_create_top_level(Some(&mut w));
            1
        }
    }
}

wrap_life_span_handler! {
    struct SpikeLife;
    impl LifeSpanHandler {
        fn on_before_close(&self, _b: Option<&mut Browser>) {
            let open = OPEN.with(|c| { let n = c.get() - 1; c.set(n); n });
            if open <= 0 { quit_message_loop(); }
        }
        fn on_after_created(&self, b: Option<&mut Browser>) {
            OPEN.with(|c| c.set(c.get() + 1));
            if let Some(b) = b {
                let expected = PENDING.with(|p| { let v = p.get(); if v > 0 { p.set(v - 1); } v > 0 });
                if expected && b.host().map(|h| h.runtime_style()) == Some(RuntimeStyle::CHROME) && MODE.with(|m| m.borrow().as_str() == "native") {
                    NATIVE.with(|n| n.borrow_mut().push(b.clone()));
                    reposition();
                }
            }
        }
    }
}

thread_local! {
    static MODE: RefCell<String> = const { RefCell::new(String::new()) };
    static WIN: RefCell<Option<Window>> = const { RefCell::new(None) };
    static SPARE: RefCell<Vec<BrowserView>> = const { RefCell::new(Vec::new()) };
}

wrap_task! {
    struct SwapTask;
    impl Task {
        fn execute(&self) {
            // Swap the visible Chrome-style view with a spare one that was created but never attached.
            WIN.with(|w| SPARE.with(|sp| {
                let w = w.borrow(); let Some(w) = w.as_ref() else { return };
                let mut spare = sp.borrow_mut();
                let Some(next) = spare.pop() else { return };
                let cur = w.child_view_at(0).unwrap();
                let cur_bv = cur.as_browser_view().unwrap();
                let mut cv = cur.clone();
                w.remove_child_view(Some(&mut cv));
                let mut nv = View::from(&next);
                w.add_child_view(Some(&mut nv));
                w.layout();
                spare.insert(0, cur_bv);
                eprintln!("SPIKE swap done; window children={}", w.child_view_count());
            }));
            let mut t = SwapTask::new();
            post_delayed_task(ThreadId::UI, Some(&mut t), 4000);
        }
    }
}

thread_local! { static OPEN: std::cell::Cell<i32> = const { std::cell::Cell::new(0) }; }

wrap_display_handler! {
    struct SpikeDisplay;
    impl DisplayHandler {
        fn on_address_change(&self, b: Option<&mut Browser>, f: Option<&mut Frame>, url: Option<&CefString>) {
            let (Some(b), Some(f)) = (b, f) else { return };
            if f.is_main() == 0 { return; }
            let url = url.map(|u| u.to_string()).unwrap_or_default();
            let ours = NATIVE.with(|n| n.borrow().iter().any(|x| x.identifier() == b.identifier()));
            if !ours && !url.is_empty() && b.host().map(|h| h.runtime_style()) == Some(RuntimeStyle::CHROME) && MODE.with(|m| m.borrow().as_str() == "native") {
                eprintln!("SPIKE adopt foreign chrome browser id={} url={url}", b.identifier());
                if let Some(h) = b.host() { h.close_browser(1); }
                // Re-open inside our window as a native child tab.
                let parent = WIN.with(|w| w.borrow().as_ref().map(|w| w.window_handle())).unwrap();
                let bounds = HOLE.with(|h| h.borrow().clone());
                let wi = WindowInfo { runtime_style: RuntimeStyle::CHROME, ..Default::default() }.set_as_child(parent, &bounds);
                let mut client = SpikeClient::new();
                PENDING.with(|p| p.set(p.get() + 1));
                browser_host_create_browser(Some(&wi), Some(&mut client), Some(&CefString::from(url.as_str())), Some(&BrowserSettings::default()), None, None);
            }
        }
    }
}

wrap_client! {
    struct SpikeClient;
    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(SpikeLife::new()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(SpikeDisplay::new()) }
    }
}

wrap_browser_process_handler! {
    struct SpikeBph;
    impl BrowserProcessHandler {
        fn default_client(&self) -> Option<Client> { Some(SpikeClient::new()) }
        fn on_context_initialized(&self) {
            let cmd = command_line_get_global().expect("cmdline");
            let toolbar = match switch(&cmd, "toolbar").as_str() {
                "none" => ChromeToolbarType::NONE,
                _ => ChromeToolbarType::NORMAL,
            };
            let mut urls: Vec<String> = switch(&cmd, "urls").split(',').filter(|s| !s.is_empty()).map(String::from).collect();
            if urls.is_empty() { urls.push("https://www.google.com/".into()); }
            let mode = switch(&cmd, "mode");
            MODE.with(|m| *m.borrow_mut() = mode.clone());
            let mk = |u: &str, style: RuntimeStyle| {
                let mut client = SpikeClient::new();
                let mut vd = SpikeView::new(toolbar, style);
                browser_view_create(Some(&mut client), Some(&CefString::from(u)), Some(&BrowserSettings::default()), None, None, Some(&mut vd)).unwrap()
            };
            match mode.as_str() {
                // Q1: two Chrome-style BrowserViews side by side in one Chrome-style window.
                "multi" => {
                    let a = mk(&urls[0], RuntimeStyle::CHROME);
                    let b = mk(urls.get(1).map(String::as_str).unwrap_or("https://example.com/"), RuntimeStyle::CHROME);
                    let mut wd = SpikeWindow::new(RefCell::new(Some(b)), RefCell::new(vec![a]));
                    window_create_top_level(Some(&mut wd));
                }
                // Q2: Alloy-style UI view + Chrome-style content view in one window.
                "mixed" | "mixed2" => {
                    let ui = mk("data:text/html,<body style='background:%23103b2c;color:white;font:16px sans-serif'><h3>Alloy UI view</h3><button onclick='document.body.style.background=\"%23222\"'>click</button></body>", RuntimeStyle::ALLOY);
                    let c = mk(&urls[0], RuntimeStyle::CHROME);
                    let mut wd = SpikeWindow::new(RefCell::new(Some(c)), RefCell::new(vec![ui]));
                    window_create_top_level(Some(&mut wd));
                }
                // Q3: keep several Chrome-style views alive but attach only one at a time.
                "swap" => {
                    let first = mk(&urls[0], RuntimeStyle::CHROME);
                    for u in urls.iter().skip(1) { let v = mk(u, RuntimeStyle::CHROME); SPARE.with(|s| s.borrow_mut().push(v)); }
                    let mut wd = SpikeWindow::new(RefCell::new(Some(first)), RefCell::new(vec![]));
                    window_create_top_level(Some(&mut wd));
                    let mut t = SwapTask::new();
                    post_delayed_task(ThreadId::UI, Some(&mut t), 4000);
                }
                "native" => {
                    let ui = mk("data:text/html,<body style='background:%23103b2c;color:white;font:16px sans-serif'><h3>Emerald UI (Alloy)</h3><input placeholder=type-here></body>", RuntimeStyle::ALLOY);
                    let mut wd = NativeWindow::new(RefCell::new(Some(ui)), urls.clone());
                    window_create_top_level(Some(&mut wd));
                }
                _ => for u in urls {
                    let bv = mk(&u, RuntimeStyle::CHROME);
                    let mut wd = SpikeWindow::new(RefCell::new(Some(bv)), RefCell::new(vec![]));
                    window_create_top_level(Some(&mut wd));
                },
            }
        }
    }
}

// ---- "native" mode: Alloy Views window for UI + Chrome-style tab browsers as native children ----

wrap_panel_delegate! {
    struct HolePanel {}
    impl ViewDelegate {
        fn on_layout_changed(&self, _v: Option<&mut View>, b: Option<&Rect>) {
            if let Some(b) = b { HOLE.with(|h| *h.borrow_mut() = b.clone()); reposition(); }
        }
    }
    impl PanelDelegate {}
}

wrap_window_delegate! {
    struct NativeWindow { ui: RefCell<Option<BrowserView>>, urls: Vec<String> }
    impl ViewDelegate {
        fn preferred_size(&self, _v: Option<&mut View>) -> Size { Size { width: 1280, height: 800 } }
    }
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(w) = window else { return };
            let layout = w.set_to_box_layout(Some(&BoxLayoutSettings { horizontal: 1, cross_axis_alignment: AxisAlignment::STRETCH, ..Default::default() })).unwrap();
            let mut ui = View::from(self.ui.borrow().as_ref().unwrap());
            w.add_child_view(Some(&mut ui));
            ui.set_size(Some(&Size { width: 260, height: 800 }));
            let mut hole = View::from(&panel_create(Some(&mut HolePanel::new())).unwrap());
            w.add_child_view(Some(&mut hole));
            layout.set_flex_for_view(Some(&mut hole), 1);
            w.show();
            w.layout();
            WIN.with(|x| *x.borrow_mut() = Some(w.clone()));
            let parent = w.window_handle();
            let bounds = HOLE.with(|h| h.borrow().clone());
            eprintln!("SPIKE parent handle={parent:?} hole={bounds:?}");
            for u in &self.urls {
                let wi = WindowInfo { runtime_style: RuntimeStyle::CHROME, ..Default::default() }.set_as_child(parent, &bounds);
                let mut client = SpikeClient::new();
                PENDING.with(|p| p.set(p.get() + 1));
                browser_host_create_browser(Some(&wi), Some(&mut client), Some(&CefString::from(u.as_str())), Some(&BrowserSettings::default()), None, None);
            }
            if std::env::var("SPIKE_TOGGLE").is_ok() {
                let mut t = ToggleTask::new();
                post_delayed_task(ThreadId::UI, Some(&mut t), 4000);
            }
        }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

thread_local! {
    static HOLE: RefCell<Rect> = RefCell::new(Rect::default());
    static NATIVE: RefCell<Vec<Browser>> = const { RefCell::new(Vec::new()) };
    static ACTIVE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PENDING: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(target_os = "linux")]
fn x11_show(win: cef::sys::cef_window_handle_t, show: bool, r: &Rect) {
    use x11_dl::xlib;
    thread_local! { static X: (xlib::Xlib, *mut xlib::Display) = {
        let x = xlib::Xlib::open().expect("libX11"); let d = unsafe { (x.XOpenDisplay)(std::ptr::null()) }; (x, d)
    }; }
    X.with(|(x, d)| unsafe {
        if show {
            (x.XMoveResizeWindow)(*d, win as _, r.x, r.y, r.width.max(1) as u32, r.height.max(1) as u32);
            (x.XMapRaised)(*d, win as _);
        } else { (x.XUnmapWindow)(*d, win as _); }
        (x.XFlush)(*d);
    });
}

fn reposition() {
    let r = HOLE.with(|h| h.borrow().clone());
    let active = ACTIVE.with(|a| a.get());
    NATIVE.with(|n| for (i, b) in n.borrow().iter().enumerate() {
        if let Some(h) = b.host() { x11_show(h.window_handle(), i == active, &r); }
    });
}

wrap_task! {
    struct ToggleTask;
    impl Task {
        fn execute(&self) {
            let n = NATIVE.with(|n| n.borrow().len()).max(1);
            ACTIVE.with(|a| a.set((a.get() + 1) % n));
            reposition();
            eprintln!("SPIKE toggled to {}", ACTIVE.with(|a| a.get()));
            let mut t = ToggleTask::new();
            post_delayed_task(ThreadId::UI, Some(&mut t), 4000);
        }
    }
}

wrap_app! {
    struct SpikeApp;
    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> { Some(SpikeBph::new()) }
    }
}

fn main() {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::new();
    let cmd = args.as_cmd_line().expect("cmdline");
    let is_browser = cmd.has_switch(Some(&CefString::from("type"))) != 1;
    let mut app = SpikeApp::new();
    let code = execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if !is_browser { std::process::exit(code); }

    let cef_dir = std::env::var("CEF_PATH").unwrap_or_default();
    let mut profile = switch(&cmd, "profile");
    if profile.is_empty() { profile = format!("{}/.cache/emerald-spike", std::env::var("HOME").unwrap_or_default()); }
    let settings = Settings {
        no_sandbox: 1,
        root_cache_path: CefString::from(profile.as_str()),
        cache_path: CefString::from(format!("{profile}/Default").as_str()),
        resources_dir_path: CefString::from(cef_dir.as_str()),
        locales_dir_path: CefString::from(format!("{cef_dir}/locales").as_str()),
        remote_debugging_port: 9222,
        log_severity: LogSeverity::WARNING,
        ..Default::default()
    };
    assert_eq!(initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()), 1);
    run_message_loop();
    shutdown();
}
