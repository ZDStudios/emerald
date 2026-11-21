//! The `emerald://` scheme: serves the browser UI and internal pages from assets embedded
//! in the binary, plus the live theme stylesheet.
//!
//! Routing:
//! * `emerald://<page>/`            → `ui/<page>.html` (`ui` → shell, `overlay`, `start`, `settings`, …)
//! * `emerald://<page>/theme.css`   → generated theme + user CSS + accessibility flags
//! * `emerald://<page>/<file>`      → `ui/<file>` (shared JS/CSS/fonts)
//!
//! The scheme is registered DISPLAY_ISOLATED, so web pages cannot link to, frame or
//! navigate to it. Only Emerald itself loads these URLs.
use crate::util::cs;
use cef::*;
use include_dir::{include_dir, Dir};
use std::sync::{Arc, Mutex, RwLock};

static UI: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../ui");
static THEME_CSS: RwLock<String> = RwLock::new(String::new());

pub fn set_theme_css(css: String) {
    if let Ok(mut g) = THEME_CSS.write() { *g = css; }
}

pub fn scheme_options() -> i32 {
    (SchemeOptions::STANDARD.get_raw() | SchemeOptions::SECURE.get_raw() | SchemeOptions::DISPLAY_ISOLATED.get_raw() | SchemeOptions::CORS_ENABLED.get_raw()) as i32
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "otf" => "font/otf",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// Look up an asset. `EMERALD_UI_DIR` (developer builds) serves from disk for fast iteration.
pub fn asset(path: &str) -> Option<Vec<u8>> {
    if path.contains("..") { return None; }
    if let Ok(dir) = std::env::var("EMERALD_UI_DIR") {
        if let Ok(b) = std::fs::read(std::path::Path::new(&dir).join(path)) { return Some(b); }
    }
    UI.get_file(path).map(|f| f.contents().to_vec())
}

/// Resolve an `emerald://` URL to (status, mime, body).
pub fn resolve(url: &str) -> (i32, &'static str, Vec<u8>) {
    let rest = url.strip_prefix("emerald://").unwrap_or("");
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    if path == "theme.css" {
        return (200, "text/css", THEME_CSS.read().map(|g| g.clone()).unwrap_or_default().into_bytes());
    }
    let file = if path.is_empty() || path == "index.html" {
        format!("{}.html", if host == "ui" { "shell" } else { host.as_str() })
    } else {
        path.to_string()
    };
    match asset(&file) {
        Some(b) => (200, mime(&file), b),
        None => (404, "text/html", format!("<!doctype html><meta charset=utf-8><title>Not found</title><link rel=stylesheet href=theme.css><link rel=stylesheet href=base.css><main class=page><h1>Not found</h1><p class=muted>emerald://{host}/{path} does not exist.</p></main>").into_bytes()),
    }
}

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' https: http: data: blob: chrome-extension:; font-src 'self' data:; media-src 'self' data: blob:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

wrap_resource_handler! {
    pub struct AssetHandler {
        status: i32,
        mime: &'static str,
        data: Arc<Vec<u8>>,
        offset: Arc<Mutex<usize>>,
    }
    impl ResourceHandler {
        fn open(&self, _request: Option<&mut Request>, handle_request: Option<&mut ::std::os::raw::c_int>, _callback: Option<&mut Callback>) -> ::std::os::raw::c_int {
            if let Some(h) = handle_request { *h = 1; }
            1
        }
        fn response_headers(&self, response: Option<&mut Response>, response_length: Option<&mut i64>, _redirect_url: Option<&mut CefString>) {
            if let Some(r) = response {
                r.set_status(self.status);
                r.set_status_text(Some(&cs(if self.status == 200 { "OK" } else { "Not Found" })));
                r.set_mime_type(Some(&cs(self.mime)));
                r.set_charset(Some(&cs("utf-8")));
                r.set_header_by_name(Some(&cs("Content-Security-Policy")), Some(&cs(CSP)), 1);
                r.set_header_by_name(Some(&cs("Cache-Control")), Some(&cs("no-store")), 1);
                r.set_header_by_name(Some(&cs("X-Content-Type-Options")), Some(&cs("nosniff")), 1);
            }
            if let Some(l) = response_length { *l = self.data.len() as i64; }
        }
        fn read(&self, data_out: *mut u8, bytes_to_read: ::std::os::raw::c_int, bytes_read: Option<&mut ::std::os::raw::c_int>, _callback: Option<&mut ResourceReadCallback>) -> ::std::os::raw::c_int {
            let mut off = self.offset.lock().unwrap();
            let remaining = self.data.len().saturating_sub(*off);
            let n = remaining.min(bytes_to_read.max(0) as usize);
            if n > 0 && !data_out.is_null() {
                unsafe { std::ptr::copy_nonoverlapping(self.data.as_ptr().add(*off), data_out, n); }
            }
            *off += n;
            if let Some(b) = bytes_read { *b = n as i32; }
            (n > 0) as i32
        }
        fn cancel(&self) {}
    }
}

wrap_scheme_handler_factory! {
    pub struct EmeraldSchemeFactory {}
    impl SchemeHandlerFactory {
        fn create(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _scheme_name: Option<&CefString>, request: Option<&mut Request>) -> Option<ResourceHandler> {
            let url = request.map(|r| crate::util::userfree(r.url())).unwrap_or_default();
            let (status, mime, data) = resolve(&url);
            Some(AssetHandler::new(status, mime, Arc::new(data), Arc::new(Mutex::new(0))))
        }
    }
}

pub fn register() {
    let mut f = EmeraldSchemeFactory::new();
    register_scheme_handler_factory(Some(&cs("emerald")), None, Some(&mut f));
}
