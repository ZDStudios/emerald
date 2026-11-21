//! Native child-window plumbing for tab browsers.
//!
//! Chrome-style CEF browsers are created as native children of the Emerald window (see
//! ARCHITECTURE.md §1), so showing, hiding, moving and stacking them is done with the
//! platform window API.
use cef::sys::cef_window_handle_t as Handle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

impl Rect {
    pub fn to_cef(self) -> cef::Rect { cef::Rect { x: self.x, y: self.y, width: self.w.max(1), height: self.h.max(1) } }
    pub fn is_empty(self) -> bool { self.w <= 0 || self.h <= 0 }
}

pub fn is_null(h: Handle) -> bool {
    #[cfg(target_os = "windows")]
    { h.is_null() }
    #[cfg(not(target_os = "windows"))]
    { h as u64 == 0 }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Handle, Rect};
    use x11_dl::xlib;
    struct X { lib: xlib::Xlib, dpy: *mut xlib::Display }
    thread_local! {
        static X11: Option<X> = {
            let lib = xlib::Xlib::open().ok()?;
            let dpy = unsafe { (lib.XOpenDisplay)(std::ptr::null()) };
            (!dpy.is_null()).then_some(X { lib, dpy })
        };
    }
    pub fn place(h: Handle, r: Option<Rect>) {
        X11.with(|x| {
            let Some(x) = x else { return };
            unsafe {
                match r {
                    Some(r) if !r.is_empty() => {
                        (x.lib.XMoveResizeWindow)(x.dpy, h as _, r.x, r.y, r.w as u32, r.h as u32);
                        (x.lib.XMapWindow)(x.dpy, h as _);
                    }
                    _ => { (x.lib.XUnmapWindow)(x.dpy, h as _); }
                }
                (x.lib.XFlush)(x.dpy);
            }
        });
    }
    pub fn raise(h: Handle) {
        X11.with(|x| if let Some(x) = x { unsafe { (x.lib.XRaiseWindow)(x.dpy, h as _); (x.lib.XFlush)(x.dpy); } });
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{Handle, Rect};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    pub fn place(h: Handle, r: Option<Rect>) {
        unsafe {
            match r {
                Some(r) if !r.is_empty() => {
                    SetWindowPos(h as _, std::ptr::null_mut(), r.x, r.y, r.w, r.h, SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
                }
                _ => { ShowWindow(h as _, SW_HIDE); }
            }
        }
    }
    pub fn raise(h: Handle) {
        unsafe { SetWindowPos(h as _, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE); }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    //! Tabs are child NSViews of the window's content view. Rects arrive in device pixels with a
    //! top-left origin; AppKit wants points, and (unless the superview is flipped) a bottom-left origin.
    use super::{Handle, Rect};
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    pub fn place(h: Handle, r: Option<Rect>) {
        let view = h as *mut AnyObject;
        unsafe {
            match r {
                Some(r) if !r.is_empty() => {
                    let sup: *mut AnyObject = msg_send![view, superview];
                    let win: *mut AnyObject = msg_send![view, window];
                    let scale: f64 = if win.is_null() { 1.0 } else { msg_send![win, backingScaleFactor] };
                    let scale = if scale > 0.0 { scale } else { 1.0 };
                    let (x, y, w, hgt) = (r.x as f64 / scale, r.y as f64 / scale, r.w as f64 / scale, r.h as f64 / scale);
                    let y = if sup.is_null() {
                        y
                    } else {
                        let flipped: Bool = msg_send![sup, isFlipped];
                        if flipped.as_bool() { y } else {
                            let f: NSRect = msg_send![sup, frame];
                            f.size.height - y - hgt
                        }
                    };
                    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, hgt));
                    let _: () = msg_send![view, setFrame: frame];
                    let _: () = msg_send![view, setHidden: Bool::NO];
                }
                _ => { let _: () = msg_send![view, setHidden: Bool::YES]; }
            }
        }
    }

    pub fn raise(h: Handle) {
        let view = h as *mut AnyObject;
        unsafe {
            let sup: *mut AnyObject = msg_send![view, superview];
            if sup.is_null() { return; }
            // NSWindowAbove = 1; a nil reference view puts it above all siblings.
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![sup, addSubview: view, positioned: 1isize, relativeTo: nil];
        }
    }

    /// CEF's window handle may be the NSWindow on some versions; tabs need its content view.
    pub fn parent_view(h: Handle) -> Handle {
        let obj = h as *mut AnyObject;
        unsafe {
            let is_window: Bool = msg_send![obj, respondsToSelector: objc2::sel!(contentView)];
            if is_window.as_bool() {
                let v: *mut AnyObject = msg_send![obj, contentView];
                if !v.is_null() { return v as Handle; }
            }
        }
        h
    }
}

/// Show `h` at `r`, or hide it when `r` is `None`.
pub fn place(h: Handle, r: Option<Rect>) { if !is_null(h) { imp::place(h, r) } }
pub fn raise(h: Handle) { if !is_null(h) { imp::raise(h) } }

/// The handle tab windows should be parented to (the content view on macOS).
pub fn parent_for(h: Handle) -> Handle {
    #[cfg(target_os = "macos")]
    { imp::parent_view(h) }
    #[cfg(not(target_os = "macos"))]
    { h }
}
