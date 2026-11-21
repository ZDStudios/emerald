//! Keyboard shortcuts. Handled in `on_pre_key_event` for every surface, so they work
//! whether focus is in a page, the shell UI or the overlay.
use crate::client::Source;
use crate::shell;
use cef::*;

const SHIFT: u32 = 1 << 1;
const CTRL: u32 = 1 << 2;
const ALT: u32 = 1 << 3;
const CMD: u32 = 1 << 7;

/// Map a key event to an Emerald command name (shared with the command bar).
pub fn command_for(mods: u32, vk: i32) -> Option<&'static str> {
    let primary = if cfg!(target_os = "macos") { mods & CMD != 0 } else { mods & CTRL != 0 };
    let shift = mods & SHIFT != 0;
    let alt = mods & ALT != 0;
    let key = |c: char| vk == c as i32;
    Some(match () {
        _ if primary && !shift && key('T') => "tab.new",
        _ if primary && shift && key('T') => "tab.reopen",
        _ if primary && !shift && (key('W') || vk == 0x73 /* F4 */) => "tab.close",
        _ if primary && !shift && key('K') => "palette",
        _ if primary && !shift && key('L') => "focus.address",
        _ if alt && !primary && key('D') => "focus.address",
        _ if vk == 0x74 /* F5 */ && primary => "nav.reload_hard",
        _ if vk == 0x74 => "nav.reload",
        _ if primary && shift && key('R') => "reader.toggle",
        _ if primary && key('R') => "nav.reload",
        _ if alt && vk == 0x25 /* Left */ => "nav.back",
        _ if alt && vk == 0x27 /* Right */ => "nav.forward",
        _ if primary && vk == 0x09 /* Tab */ && shift => "tab.prev",
        _ if primary && vk == 0x09 => "tab.next",
        _ if primary && (0x31..=0x38).contains(&vk) => ["tab.n1", "tab.n2", "tab.n3", "tab.n4", "tab.n5", "tab.n6", "tab.n7", "tab.n8"][(vk - 0x31) as usize],
        _ if primary && vk == 0x39 => "tab.last",
        _ if primary && !shift && key('F') => "find",
        _ if vk == 0x7B /* F12 */ || (primary && shift && key('I')) => "devtools",
        _ if primary && !shift && key('P') => "print",
        _ if primary && (vk == 0xBB || vk == 0x6B) => "zoom.in",
        _ if primary && (vk == 0xBD || vk == 0x6D) => "zoom.out",
        _ if primary && (vk == 0x30 || vk == 0x60) => "zoom.reset",
        _ if primary && shift && key('F') => "focus.toggle",
        _ if primary && shift && key('S') => "sidebar.toggle",
        _ if primary && shift && key('P') => "tab.park",
        _ if primary && shift && key('N') => "note.quick",
        _ if primary && shift && key('M') => "memory",
        _ if primary && shift && key('E') => "split.toggle",
        _ if primary && key('H') => "history",
        _ if primary && key('J') => "downloads",
        _ if primary && key('D') => "bookmark.add",
        _ if primary && key(',') || (primary && vk == 0xBC) => "settings",
        _ if vk == 0x7A /* F11 */ => "fullscreen",
        _ if cfg!(target_os = "macos") && primary && key('Q') => "app.quit",
        _ => return None,
    })
}

pub fn handle(src: Source, e: &KeyEvent) -> bool {
    if e.type_ != KeyEventType::RAWKEYDOWN { return false; }
    // Escape closes the overlay (pages keep their Escape otherwise).
    if e.windows_key_code == 0x1B && src == Source::Overlay {
        shell::with(|s| s.hide_overlay());
        return true;
    }
    let Some(cmd) = command_for(e.modifiers, e.windows_key_code) else { return false };
    // Let the address bar and overlay text fields keep plain editing keys.
    shell::with(move |s| { let _ = crate::api::command(s, cmd, &serde_json::Value::Null); });
    true
}

/// Chrome commands raised by Chrome internals (menus, accelerators we didn't catch).
/// Return true when Emerald handled (or blocked) the command.
pub fn chrome_command(_src: Source, id: i32) -> bool {
    let cmd = match id {
        34000 | 34001 | 34014 => "tab.new",      // NEW_WINDOW, NEW_INCOGNITO_WINDOW, NEW_TAB
        34015 => "tab.close",                    // CLOSE_TAB
        34012 | 34031 => "noop",                 // CLOSE_WINDOW, EXIT: handled by our window
        34016 => "tab.next",
        34017 => "tab.prev",
        34028 => "tab.reopen",
        40010 => "history",
        40012 => "downloads",
        40015 => "settings",
        _ => {
            if std::env::var("EMERALD_DEBUG").is_ok() { eprintln!("emerald: chrome command {id} passed through"); }
            return false;
        }
    };
    shell::with(move |s| { let _ = crate::api::command(s, cmd, &serde_json::Value::Null); });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts() {
        assert_eq!(command_for(CTRL, 'T' as i32), Some("tab.new"));
        assert_eq!(command_for(CTRL | SHIFT, 'T' as i32), Some("tab.reopen"));
        assert_eq!(command_for(CTRL, 'K' as i32), Some("palette"));
        assert_eq!(command_for(CTRL | SHIFT, 0x09), Some("tab.prev"));
        assert_eq!(command_for(CTRL, 0x33), Some("tab.n3"));
        assert_eq!(command_for(0, 'K' as i32), None);
        assert_eq!(command_for(ALT, 0x25), Some("nav.back"));
    }
}
