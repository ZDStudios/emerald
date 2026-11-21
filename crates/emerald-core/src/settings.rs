//! User settings. Every field has a default, so older or partial files keep loading
//! (`#[serde(default)]` on every struct).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub first_run_done: bool,
    pub theme: String,
    /// Extra CSS appended to every Emerald UI surface (shell, overlay, internal pages).
    pub custom_css: String,
    pub layout: Layout,
    pub search: Search,
    pub startup: Startup,
    pub memory: Memory,
    pub privacy: Privacy,
    pub adhd: Adhd,
    pub start_page: StartPage,
    pub downloads_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            first_run_done: false,
            theme: "emerald-dark".into(),
            custom_css: String::new(),
            layout: Layout::default(),
            search: Search::default(),
            startup: Startup::default(),
            memory: Memory::default(),
            privacy: Privacy::default(),
            adhd: Adhd::default(),
            start_page: StartPage::default(),
            downloads_dir: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TabLayout { Vertical, Horizontal }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Density { Compact, Comfortable, Spacious }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Layout {
    pub tabs: TabLayout,
    pub sidebar_width: u32,
    pub sidebar_right: bool,
    pub sidebar_autohide: bool,
    pub topbar_autohide: bool,
    pub density: Density,
    pub show_window_controls: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self { tabs: TabLayout::Vertical, sidebar_width: 240, sidebar_right: false, sidebar_autohide: false, topbar_autohide: false, density: Density::Comfortable, show_window_controls: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Search {
    pub engine: String,
    /// `%s` is replaced by the URL-encoded query.
    pub url: String,
}

impl Default for Search {
    fn default() -> Self { Self { engine: "DuckDuckGo".into(), url: "https://duckduckgo.com/?q=%s".into() } }
}

pub const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    ("Google", "https://www.google.com/search?q=%s"),
    ("Bing", "https://www.bing.com/search?q=%s"),
    ("Brave", "https://search.brave.com/search?q=%s"),
    ("Startpage", "https://www.startpage.com/do/search?q=%s"),
    ("Ecosia", "https://www.ecosia.org/search?q=%s"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Startup {
    pub restore_session: bool,
}
impl Default for Startup { fn default() -> Self { Self { restore_session: true } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Memory {
    /// Put background tabs to sleep after this many minutes (0 = never).
    pub sleep_after_minutes: u32,
    /// Only the active tab is created on startup; others wake on click.
    pub lazy_restore: bool,
    /// Chromium `--renderer-process-limit` (0 = Chromium default).
    pub renderer_process_limit: u32,
    pub never_sleep_pinned: bool,
    pub never_sleep_audible: bool,
}
impl Default for Memory {
    fn default() -> Self { Self { sleep_after_minutes: 15, lazy_restore: true, renderer_process_limit: 0, never_sleep_pinned: false, never_sleep_audible: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Privacy {
    pub block_ads_trackers: bool,
    pub https_first: bool,
    pub block_third_party_cookies: bool,
    pub send_do_not_track: bool,
}
impl Default for Privacy {
    fn default() -> Self { Self { block_ads_trackers: true, https_first: true, block_third_party_cookies: true, send_do_not_track: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FocusSchedule {
    pub name: String,
    /// Monday = 0 … Sunday = 6.
    pub days: [bool; 7],
    /// Minutes after local midnight.
    pub start_min: u32,
    pub end_min: u32,
    pub enabled: bool,
}
impl Default for FocusSchedule {
    fn default() -> Self { Self { name: "Deep work".into(), days: [true, true, true, true, true, false, false], start_min: 9 * 60, end_min: 12 * 60, enabled: false } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Pomodoro {
    pub work_min: u32,
    pub short_break_min: u32,
    pub long_break_min: u32,
    pub cycles_before_long: u32,
    /// Block distracting sites while a work interval runs.
    pub block_during_work: bool,
}
impl Default for Pomodoro {
    fn default() -> Self { Self { work_min: 25, short_break_min: 5, long_break_min: 15, cycles_before_long: 4, block_during_work: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Adhd {
    pub enabled: bool,
    /// Gentle nudge when more tabs than this are open in a space (0 = off).
    pub tab_limit: u32,
    pub reduced_motion: bool,
    pub dyslexia_font: bool,
    pub big_targets: bool,
    /// Everything quiet by default: no toasts except ones the user asked for (timers, reminders).
    pub quiet: bool,
    pub blocked_sites: Vec<String>,
    pub schedules: Vec<FocusSchedule>,
    pub pomodoro: Pomodoro,
    /// After this many minutes on a blocked-list site outside focus time, show a soft nudge (0 = off).
    pub distraction_nudge_min: u32,
}
impl Default for Adhd {
    fn default() -> Self {
        Self {
            enabled: true,
            tab_limit: 0,
            reduced_motion: false,
            dyslexia_font: false,
            big_targets: false,
            quiet: true,
            blocked_sites: vec!["youtube.com".into(), "reddit.com".into(), "x.com".into(), "twitter.com".into(), "instagram.com".into(), "tiktok.com".into(), "facebook.com".into()],
            schedules: vec![FocusSchedule::default()],
            pomodoro: Pomodoro::default(),
            distraction_nudge_min: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Widget {
    pub id: String,
    /// clock | search | links | notes | focus | parked | quote | memory
    pub kind: String,
    /// Grid placement (12 columns).
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub config: serde_json::Value,
}
impl Default for Widget {
    fn default() -> Self { Self { id: String::new(), kind: "clock".into(), x: 0, y: 0, w: 4, h: 2, config: serde_json::Value::Null } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StartPage {
    pub greeting: bool,
    /// CSS background (color/gradient) or `image:<data-url>`; empty = theme default.
    pub background: String,
    pub widgets: Vec<Widget>,
}
impl Default for StartPage {
    fn default() -> Self {
        let w = |id: &str, kind: &str, x, y, wd, h| Widget { id: id.into(), kind: kind.into(), x, y, w: wd, h, config: serde_json::Value::Null };
        Self {
            greeting: true,
            background: String::new(),
            widgets: vec![
                w("clock", "clock", 0, 0, 12, 2),
                w("search", "search", 2, 2, 8, 1),
                Widget {
                    config: serde_json::json!({"links": [
                        {"title": "Wikipedia", "url": "https://www.wikipedia.org/"},
                        {"title": "GitHub", "url": "https://github.com/"},
                        {"title": "MDN", "url": "https://developer.mozilla.org/"},
                        {"title": "Hacker News", "url": "https://news.ycombinator.com/"}
                    ]}),
                    ..w("links", "links", 2, 3, 8, 2)
                },
                w("focus", "focus", 2, 5, 4, 3),
                w("notes", "notes", 6, 5, 4, 3),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings = serde_json::from_str(r#"{"theme":"paper","adhd":{"tab_limit":8}}"#).unwrap();
        assert_eq!(s.theme, "paper");
        assert_eq!(s.adhd.tab_limit, 8);
        assert!(s.adhd.quiet);
        assert_eq!(s.memory.sleep_after_minutes, 15);
    }
    #[test]
    fn roundtrip() {
        let s = Settings::default();
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, back);
    }
}
