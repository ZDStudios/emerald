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
    }
}
