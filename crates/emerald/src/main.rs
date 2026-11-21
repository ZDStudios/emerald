//! Emerald: a minimal, customizable, ADHD-friendly browser.
//! A Rust shell over Chromium (CEF). See ARCHITECTURE.md.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod api;
mod app;
mod bookmarks;
mod client;
mod downloads;
mod extensions;
mod focus;
mod history;
mod icon;
mod import;
mod keys;
mod memory;
mod menus;
mod native;
mod nav;
mod passwords;
mod privacy;
mod reader;
mod scheme;
mod shell;
mod util;
mod window;

use cef::*;
use emerald_core::paths::Paths;
use std::path::PathBuf;

fn switch(cmd: &CommandLine, name: &str) -> String {
    CefString::from(&cmd.switch_value(Some(&CefString::from(name)))).to_string()
}

/// Directory holding libcef, *.pak, icudtl.dat and locales/. In a packaged build that is
/// the executable's directory; developers can point `CEF_PATH` at an exported CEF.
fn cef_dir() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    if exe_dir.join("icudtl.dat").exists() { return exe_dir; }
    std::env::var_os("CEF_PATH").map(PathBuf::from).unwrap_or(exe_dir)
}

fn main() {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::new();
    let Some(cmd) = args.as_cmd_line() else { std::process::exit(1) };
    let is_browser = cmd.has_switch(Some(&CefString::from("type"))) == 0;
    let mut app = app::EmeraldApp::new();

    if is_browser {
        let profile = switch(&cmd, "profile");
        let paths = Paths::new((!profile.is_empty()).then(|| PathBuf::from(profile)));
        let _ = app::BOOT.set(emerald_core::store::load_json(&paths.file("settings.json")));
        let _ = app::PATHS.set(paths);
    }

    // Sub-processes (renderer, GPU, utility) run here and exit.
    let code = execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if !is_browser { std::process::exit(code.max(0)); }

    let paths = app::PATHS.get().expect("paths");
    let dir = cef_dir();
    let s = |p: PathBuf| CefString::from(p.to_string_lossy().as_ref());
    let settings = Settings {
        // The Linux SUID/namespace sandbox needs packaging support (see README); Windows
        // release builds enable the sandbox through bootstrap.exe.
        no_sandbox: 1,
        root_cache_path: s(paths.engine()),
        cache_path: s(paths.engine_profile()),
        resources_dir_path: s(dir.clone()),
        locales_dir_path: s(dir.join("locales")),
        persist_session_cookies: 1,
        log_severity: if std::env::var("EMERALD_DEBUG").is_ok() { LogSeverity::INFO } else { LogSeverity::ERROR },
        background_color: 0xff0e_1512,
        user_agent_product: CefString::from(""),
        ..Default::default()
    };
    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) != 1 {
        eprintln!("emerald: CEF failed to initialize (is another Emerald using this profile?)");
        std::process::exit(1);
    }
    run_message_loop();
    shutdown();
}
