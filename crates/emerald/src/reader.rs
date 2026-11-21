//! Reader mode and distraction-free video mode, injected into the page.
use cef::*;

const READER_JS: &str = include_str!("../../../ui/inject/reader.js");

/// Hide everything on a video page except the player (YouTube and generic <video>).
pub const VIDEO_FOCUS_JS: &str = include_str!("../../../ui/inject/video-focus.js");

/// Inject reader mode. With the dyslexia setting on, the bundled OpenDyslexic font is
/// passed in as a data URL (web pages cannot load emerald:// resources) and preselected.
pub fn inject(b: &Browser, dys: bool) {
    if dys {
        if let Some(font) = crate::scheme::asset("fonts/OpenDyslexic-Regular.woff2") {
            use base64::Engine;
            let url = format!("data:font/woff2;base64,{}", base64::engine::general_purpose::STANDARD.encode(font));
            crate::util::exec_js(b, &format!("window.__emDysFont={};", serde_json::to_string(&url).unwrap()));
        }
    }
    crate::util::exec_js(b, READER_JS);
}
