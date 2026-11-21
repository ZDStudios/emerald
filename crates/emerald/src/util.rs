//! Small CEF helpers: closures as tasks, string conversion, JS escaping.
use cef::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

wrap_task! {
    pub struct SendTask {
        f: Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>,
    }
    impl Task {
        fn execute(&self) {
            let f = self.f.lock().ok().and_then(|mut g| g.take());
            if let Some(f) = f { f(); }
        }
    }
}

wrap_task! {
    pub struct LocalTask {
        f: Rc<RefCell<Option<Box<dyn FnOnce()>>>>,
    }
    impl Task {
        fn execute(&self) {
            let f = self.f.borrow_mut().take();
            if let Some(f) = f { f(); }
        }
    }
}

/// Run `f` on the browser UI thread (from any thread).
pub fn post_ui(f: impl FnOnce() + Send + 'static) {
    let mut t = SendTask::new(Arc::new(Mutex::new(Some(Box::new(f)))));
    post_task(ThreadId::UI, Some(&mut t));
}

/// Run `f` on the UI thread after `ms` milliseconds. Must be called on the UI thread
/// (the closure need not be `Send`).
pub fn later(ms: i64, f: impl FnOnce() + 'static) {
    let mut t = LocalTask::new(Rc::new(RefCell::new(Some(Box::new(f)))));
    post_delayed_task(ThreadId::UI, Some(&mut t), ms);
}

/// The native event passed to keyboard handlers differs per platform.
#[cfg(target_os = "linux")]
pub type OsEvent<'a> = Option<&'a mut cef::sys::XEvent>;
#[cfg(target_os = "windows")]
pub type OsEvent<'a> = Option<&'a mut cef::sys::MSG>;
#[cfg(target_os = "macos")]
pub type OsEvent<'a> = *mut u8;

pub fn cs(s: &str) -> CefString { CefString::from(s) }

pub fn opt_str(s: Option<&CefString>) -> String { s.map(|s| s.to_string()).unwrap_or_default() }

pub fn userfree(s: CefStringUserfree) -> String { CefString::from(&s).to_string() }

/// Main-frame URL of a browser.
pub fn browser_url(b: &Browser) -> String {
    b.main_frame().map(|f| userfree(f.url())).unwrap_or_default()
}

pub fn exec_js(b: &Browser, code: &str) {
    if let Some(f) = b.main_frame() {
        f.execute_java_script(Some(&cs(code)), Some(&cs("emerald://internal/")), 0);
    }
}

pub fn on_ui_thread() -> bool { currently_on(ThreadId::UI) != 0 }
