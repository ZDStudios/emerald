//! The top-level Emerald window (CEF Views, Alloy style) hosting the shell UI view.
use crate::shell;
use cef::*;
use std::cell::RefCell;

wrap_browser_view_delegate! {
    pub struct UiViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

wrap_window_delegate! {
    pub struct MainWindow {
        ui: RefCell<Option<BrowserView>>,
        frameless: bool,
    }
    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size { Size { width: 1280, height: 820 } }
        fn minimum_size(&self, _view: Option<&mut View>) -> Size { Size { width: 520, height: 360 } }
    }
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let (Some(w), Some(ui)) = (window, self.ui.borrow().clone()) else { return };
            w.set_to_fill_layout();
            let mut v = View::from(&ui);
            w.add_child_view(Some(&mut v));
            w.set_title(Some(&crate::util::cs("Emerald")));
            if let Some(icon) = crate::icon::window_icon() { let mut i = icon; w.set_window_app_icon(Some(&mut i)); }
            w.show();
            v.request_focus();
            let w2 = w.clone();
            shell::with(move |s| { s.window = Some(w2); s.apply_layout(); });
        }
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            *self.ui.borrow_mut() = None;
            quit_message_loop();
        }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            shell::query(|s| s.request_close()).unwrap_or(true) as _
        }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { self.frameless as _ }
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn initial_show_state(&self, _window: Option<&mut Window>) -> ShowState {
            if std::env::args().any(|a| a == "--maximized") { ShowState::MAXIMIZED } else { ShowState::NORMAL }
        }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
        fn linux_window_properties(&self, _window: Option<&mut Window>, properties: Option<&mut LinuxWindowProperties>) -> ::std::os::raw::c_int {
            let Some(p) = properties else { return 0 };
            p.wayland_app_id = crate::util::cs("emerald");
            p.wm_class_class = crate::util::cs("Emerald");
            p.wm_class_name = crate::util::cs("emerald");
            1
        }
    }
}

pub fn create(frameless: bool) {
    let ui = crate::client::ui_browser_view("emerald://ui/");
    let mut d = MainWindow::new(RefCell::new(ui), frameless);
    window_create_top_level(Some(&mut d));
}
