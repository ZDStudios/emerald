//! macOS glue: CEF requires the application object to be an `NSApplication` subclass that
//! implements its `CefAppProtocol` (it tracks whether an event is being dispatched).
use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::runtime::{AnyObject, Bool};
use objc2::{define_class, msg_send, ClassType, DefinedClass, MainThreadMarker};
use objc2_app_kit::{NSApp, NSApplication, NSEvent};
use std::cell::Cell;

#[derive(Default)]
pub struct EmeraldApplicationIvars {
    handling_send_event: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[ivars = EmeraldApplicationIvars]
    pub struct EmeraldApplication;

    impl EmeraldApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            let was = self.ivars().handling_send_event.get();
            if !was { self.ivars().handling_send_event.set(true); }
            let _: () = msg_send![super(self), sendEvent: event];
            if !was { self.ivars().handling_send_event.set(false); }
        }

        /// Cmd+Q, the Dock menu and logout end up here. Chromium must leave its run loop in an
        /// orderly way, so close the window (which closes the tabs and quits the loop) instead of
        /// letting AppKit call exit().
        #[unsafe(method(terminate:))]
        unsafe fn terminate(&self, _sender: Option<&AnyObject>) {
            crate::shell::with(|s| if let Some(w) = &s.window { w.close() });
        }
    }

    unsafe impl CrAppControlProtocol for EmeraldApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn set_handling_send_event(&self, handling: Bool) {
            self.ivars().handling_send_event.set(handling.as_bool());
        }
    }

    unsafe impl CrAppProtocol for EmeraldApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn is_handling_send_event(&self) -> Bool {
            Bool::new(self.ivars().handling_send_event.get())
        }
    }

    unsafe impl CefAppProtocol for EmeraldApplication {}
);

/// Make `EmeraldApplication` the shared application. Must run before anything touches NSApp.
pub fn setup_application() {
    let _: *mut AnyObject = unsafe { msg_send![EmeraldApplication::class(), sharedApplication] };
    let mtm = MainThreadMarker::new().expect("Emerald must start on the main thread");
    assert!(NSApp(mtm).isKindOfClass(EmeraldApplication::class()), "NSApp was created before EmeraldApplication");
}
