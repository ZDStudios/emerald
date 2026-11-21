//! Page context menu: remove items that would open stock Chrome windows, add Emerald ones.
use crate::shell;
use crate::util::{cs, userfree};
use cef::*;
use emerald_core::session::Id;
use std::os::raw::c_int;

const OPEN_IN_SPLIT: c_int = 26501; // MENU_ID_USER_FIRST (26500) + n
const PARK_TAB: c_int = 26502;
const READER: c_int = 26503;
const NOTE_SELECTION: c_int = 26504;
const OPEN_BG_TAB: c_int = 26505;
const COPY_LINK: c_int = 26506;
const INSPECT: c_int = 26507;

// Chrome context-menu command ids for "open link in new window / incognito window".
const CHROME_OPEN_NEW_WINDOW: c_int = 50101;
const CHROME_OPEN_INCOGNITO: c_int = 50102;

wrap_context_menu_handler! {
    pub struct Menus { tab: Id }
    impl ContextMenuHandler {
        fn on_before_context_menu(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, params: Option<&mut ContextMenuParams>, model: Option<&mut MenuModel>) {
            let (Some(p), Some(m)) = (params, model) else { return };
            m.remove(CHROME_OPEN_NEW_WINDOW);
            m.remove(CHROME_OPEN_INCOGNITO);
            let link = userfree(p.link_url());
            let sel = userfree(p.selection_text());
            m.add_separator();
            if !link.is_empty() {
                m.add_item(OPEN_BG_TAB, Some(&cs("Open link in background tab")));
                m.add_item(OPEN_IN_SPLIT, Some(&cs("Open link in split view")));
                m.add_item(COPY_LINK, Some(&cs("Copy link address")));
            }
            if !sel.trim().is_empty() {
                m.add_item(NOTE_SELECTION, Some(&cs("Save selection to quick notes")));
            }
            m.add_item(READER, Some(&cs("Reader mode")));
            m.add_item(PARK_TAB, Some(&cs("Park this tab for later…")));
            m.add_item(INSPECT, Some(&cs("Inspect")));
        }
        fn on_context_menu_command(&self, browser: Option<&mut Browser>, _frame: Option<&mut Frame>, params: Option<&mut ContextMenuParams>, command_id: c_int, _event_flags: EventFlags) -> c_int {
            let Some(p) = params else { return 0 };
            let link = userfree(p.link_url());
            let sel = userfree(p.selection_text());
            let page = userfree(p.page_url());
            let (x, y) = (p.xcoord(), p.ycoord());
            let tab = self.tab;
            match command_id {
                OPEN_BG_TAB => shell::with(move |s| { s.new_tab(Some(&link), Some(tab), true); }),
                OPEN_IN_SPLIT => shell::with(move |s| {
                    let id = s.new_tab(Some(&link), Some(tab), true);
                    s.session.split(tab, id);
                    s.after_tab_change();
                }),
                COPY_LINK => shell::with(move |s| s.emit_ui("copy", serde_json::json!(link))),
                NOTE_SELECTION => shell::with(move |s| crate::api::add_note(s, &sel, &page)),
                READER => shell::with(move |s| { let _ = crate::api::command(s, "reader.toggle", &serde_json::Value::Null); }),
                PARK_TAB => shell::with(move |s| s.show_overlay("park", serde_json::json!({"tab": tab}))),
                INSPECT => {
                    if let Some(h) = browser.and_then(|b| b.host()) {
                        h.show_dev_tools(Some(&WindowInfo::default()), None, None, Some(&Point { x, y }));
                    }
                }
                _ => return 0,
            }
            1
        }
    }
}

pub fn handler(tab: Id) -> ContextMenuHandler { Menus::new(tab) }
