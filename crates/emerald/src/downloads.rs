//! Downloads: saved straight to the downloads folder (no dialog), tracked for the
//! downloads page and the sidebar indicator.
use crate::shell;
use crate::util::{cs, opt_str, userfree};
use cef::*;
use serde::Serialize;
use std::os::raw::c_int;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct Download {
    pub id: u32,
    pub url: String,
    pub name: String,
    pub path: String,
    pub received: i64,
    pub total: i64,
    pub speed: i64,
    /// in_progress | complete | canceled | interrupted | paused
    pub state: String,
    pub started: u64,
    pub dismissed: bool,
}

thread_local! {
    static CONTROLS: std::cell::RefCell<std::collections::HashMap<u32, DownloadItemCallback>> = Default::default();
}

pub fn control(id: u32, action: &str) -> bool {
    CONTROLS.with(|c| c.borrow().get(&id).map(|cb| match action {
        "pause" => cb.pause(),
        "resume" => cb.resume(),
        _ => cb.cancel(),
    }).is_some())
}

fn unique_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let clean: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = if clean.trim().is_empty() { "download".to_string() } else { clean };
    let p = dir.join(&clean);
    if !p.exists() { return p; }
    let (stem, ext) = match clean.rsplit_once('.') { Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")), _ => (clean.clone(), String::new()) };
    (1..1000).map(|i| dir.join(format!("{stem} ({i}){ext}"))).find(|p| !p.exists()).unwrap_or(p)
}

wrap_download_handler! {
    pub struct Downloads {}
    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _request_method: Option<&CefString>) -> c_int { 1 }
        fn on_before_download(&self, _browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, suggested_name: Option<&CefString>, callback: Option<&mut BeforeDownloadCallback>) -> c_int {
            let (Some(item), Some(cb)) = (item, callback) else { return 0 };
            let dir = shell::query(|s| s.settings.downloads_dir.clone()).flatten().map(PathBuf::from)
                .or_else(dirs_download).unwrap_or_else(std::env::temp_dir);
            let _ = std::fs::create_dir_all(&dir);
            let path = unique_path(&dir, &opt_str(suggested_name));
            cb.cont(Some(&cs(&path.to_string_lossy())), 0);
            let d = Download { id: item.id(), url: userfree(item.url()), name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), path: path.to_string_lossy().into_owned(), received: 0, total: item.total_bytes(), speed: 0, state: "in_progress".into(), started: emerald_core::now_ms(), dismissed: false };
            shell::with(move |s| { s.downloads.retain(|x| x.id != d.id); s.downloads.insert(0, d); s.push_state(); s.emit_all("downloads", serde_json::json!(s.downloads)); });
            1
        }
        fn on_download_updated(&self, _browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, callback: Option<&mut DownloadItemCallback>) {
            let Some(item) = item else { return };
            let id = item.id();
            if let Some(cb) = callback { let cb = cb.clone(); CONTROLS.with(|c| { c.borrow_mut().insert(id, cb); }); }
            let state = if item.is_complete() != 0 { "complete" } else if item.is_canceled() != 0 { "canceled" } else if item.is_interrupted() != 0 { "interrupted" } else if item.is_paused() != 0 { "paused" } else { "in_progress" };
            let (received, total, speed, path) = (item.received_bytes(), item.total_bytes(), item.current_speed(), userfree(item.full_path()));
            if state != "in_progress" && state != "paused" { CONTROLS.with(|c| { c.borrow_mut().remove(&id); }); }
            shell::with(move |s| {
                if let Some(d) = s.downloads.iter_mut().find(|d| d.id == id) {
                    d.received = received; d.total = total; d.speed = speed; d.state = state.into();
                    if !path.is_empty() { d.path = path; }
                }
                s.emit_all("downloads", serde_json::json!(s.downloads));
                if state == "complete" { s.push_state(); }
            });
        }
    }
}

fn dirs_download() -> Option<PathBuf> {
    std::env::var_os("XDG_DOWNLOAD_DIR").map(PathBuf::from).or_else(|| {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
        Some(PathBuf::from(home).join("Downloads"))
    })
}

pub fn handler() -> DownloadHandler { Downloads::new() }
