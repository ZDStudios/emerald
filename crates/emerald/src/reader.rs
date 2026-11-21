//! Reader mode and distraction-free video mode, injected into the page.
use cef::*;

const READER_JS: &str = include_str!("../../../ui/inject/reader.js");

/// Hide everything on a video page except the player (YouTube and generic <video>).
pub const VIDEO_FOCUS_JS: &str = include_str!("../../../ui/inject/video-focus.js");

pub fn inject(b: &Browser) { crate::util::exec_js(b, READER_JS); }
