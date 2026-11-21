//! CEF application: process-wide switches, the `emerald://` scheme, and startup.
use crate::util::cs;
use cef::*;
use emerald_core::paths::Paths;
use emerald_core::settings::Settings;
use std::sync::OnceLock;

pub static PATHS: OnceLock<Paths> = OnceLock::new();
pub static BOOT: OnceLock<Settings> = OnceLock::new();

wrap_app! {
    pub struct EmeraldApp {}
    impl App {
        fn on_before_command_line_processing(&self, process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
            let Some(cl) = command_line else { return };
            if process_type.is_some_and(|p| !p.to_string().is_empty()) { return; }
            let has = |n: &str| cl.has_switch(Some(&cs(n))) != 0;
            // Tabs are native child windows; Wayland cannot host foreign child surfaces,
            // so on Linux Emerald always uses X11 (XWayland on Wayland desktops).
            #[cfg(target_os = "linux")]
            if !has("ozone-platform") { cl.append_switch_with_value(Some(&cs("ozone-platform")), Some(&cs("x11"))); }
            for s in ["no-first-run", "no-default-browser-check", "disable-search-engine-choice-screen", "no-startup-window", "disable-session-crashed-bubble"] {
                if !has(s) { cl.append_switch(Some(&cs(s))); }
            }
            if let Some(b) = BOOT.get() {
                if b.memory.renderer_process_limit > 0 && !has("renderer-process-limit") {
                    cl.append_switch_with_value(Some(&cs("renderer-process-limit")), Some(&cs(&b.memory.renderer_process_limit.to_string())));
                }
            }
        }
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(r) = registrar { r.add_custom_scheme(Some(&cs("emerald")), crate::scheme::scheme_options()); }
        }
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> { Some(Bph::new()) }
    }
}

wrap_browser_process_handler! {
    pub struct Bph {}
    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            crate::scheme::register();
            let paths = PATHS.get().expect("paths").clone();
            crate::privacy::load_decisions(&paths.file("permissions.json"));
            let shell = crate::shell::Shell::new(paths);
            crate::privacy::configure(shell.settings.privacy.block_ads_trackers, shell.settings.privacy.send_do_not_track);
            let report = crate::privacy::apply_prefs(&shell.settings);
            if std::env::var("EMERALD_DEBUG").is_ok() { eprintln!("emerald: prefs {report:?}"); }
            let frameless = !std::env::args().any(|a| a == "--native-frame");
            crate::shell::install(shell);
            crate::window::create(frameless);
            crate::nav::start_housekeeping();
        }
        fn default_client(&self) -> Option<Client> { Some(crate::client::foreign_client()) }
        fn on_already_running_app_relaunch(&self, command_line: Option<&mut CommandLine>, current_directory: Option<&CefString>) -> ::std::os::raw::c_int {
            // A second launch (e.g. a link clicked in another app) is forwarded here by
            // Chromium's process singleton: open its URLs as tabs in this window.
            let mut args = CefStringList::new();
            if let Some(cl) = command_line { cl.arguments(Some(&mut args)); }
            let cwd = current_directory.map(|c| c.to_string()).unwrap_or_default();
            let urls = launch_targets(args.into_iter().collect(), &cwd);
            crate::shell::with(move |s| {
                for u in &urls { let id = s.new_tab(Some(u), None, false); s.activate(id); }
                if let Some(w) = &s.window { if w.is_minimized() != 0 { w.restore(); } w.activate(); w.bring_to_top(); }
            });
            1
        }
    }
}

/// URLs to open from command-line arguments: http(s)/file/emerald URLs and local files.
pub fn launch_targets(args: Vec<String>, cwd: &str) -> Vec<String> {
    args.into_iter().filter(|a| !a.starts_with('-')).filter_map(|a| {
        let lower = a.to_ascii_lowercase();
        if ["http://", "https://", "file://", "emerald://"].iter().any(|p| lower.starts_with(p)) { return Some(a); }
        let p = std::path::Path::new(&a);
        let p = if p.is_absolute() { p.to_path_buf() } else { std::path::Path::new(cwd).join(p) };
        p.exists().then(|| {
            let s = p.to_string_lossy().replace('\\', "/");
            if s.starts_with('/') { format!("file://{s}") } else { format!("file:///{s}") }
        })
    }).collect()
}
