//! Themes: JSON files mapped onto CSS custom properties.
//!
//! A theme can come from a stranger (imported packs), so every value is validated
//! (no `;{}<>` breakouts), and theme CSS from imports may not load remote resources (a
//! tracking pixel in a theme would leak when the theme renders).
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub author: String,
    /// "light" | "dark": tells pages and scrollbars which color-scheme to use.
    pub mode: String,
    pub colors: BTreeMap<String, String>,
    pub font_ui: String,
    pub font_mono: String,
    /// Corner radius in px.
    pub radius: u32,
    /// compact | comfortable | spacious
    pub density: String,
    /// Optional layout preference the theme ships with ("vertical" / "horizontal").
    pub tab_layout: Option<String>,
    /// CSS background for the start page.
    pub start_background: String,
    pub css: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            id: "custom".into(),
            name: "Custom".into(),
            author: String::new(),
            mode: "dark".into(),
            colors: BTreeMap::new(),
            font_ui: "\"Inter\", \"Segoe UI Variable Text\", \"Segoe UI\", system-ui, sans-serif".into(),
            font_mono: "\"JetBrains Mono\", \"Cascadia Code\", Consolas, ui-monospace, monospace".into(),
            radius: 10,
            density: "comfortable".into(),
            tab_layout: None,
            start_background: String::new(),
            css: String::new(),
            builtin: false,
        }
    }
}

/// Color slots every theme should define (missing ones fall back to Emerald Dark).
pub const COLOR_KEYS: &[&str] = &[
    "bg", "surface", "surface-2", "hover", "active", "border", "text", "text-muted", "accent", "accent-text", "accent-soft", "danger", "warn", "success", "focus-ring", "shadow",
];

const BUILTIN: &[&str] = &[
    include_str!("../../../themes/emerald-dark.json"),
    include_str!("../../../themes/emerald-light.json"),
    include_str!("../../../themes/midnight.json"),
    include_str!("../../../themes/paper.json"),
    include_str!("../../../themes/lagoon.json"),
    include_str!("../../../themes/dusk.json"),
    include_str!("../../../themes/high-contrast.json"),
];

pub fn builtins() -> Vec<Theme> {
    BUILTIN.iter().map(|s| { let mut t: Theme = serde_json::from_str(s).expect("built-in theme JSON"); t.builtin = true; t }).collect()
}

fn safe_value(v: &str) -> bool {
    !v.is_empty() && v.len() < 4000 && !v.contains([';', '{', '}', '<', '>', '\\', '\n', '\r']) && !v.to_ascii_lowercase().contains("expression(")
}

fn safe_id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Strip remote loads from CSS: `@import` lines and `url(...)` that isn't `data:`.
pub fn sanitize_imported_css(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let lower = css.to_ascii_lowercase();
    let mut i = 0;
    while i < css.len() {
        if lower[i..].starts_with("@import") {
            i += lower[i..].find(';').map(|j| j + 1).unwrap_or(css.len() - i);
            continue;
        }
        if lower[i..].starts_with("url(") {
            let close = lower[i..].find(')').map(|j| i + j + 1).unwrap_or(css.len());
            let inner = lower[i + 4..close.saturating_sub(1).max(i + 4)].trim().trim_matches(['"', '\'']);
            if inner.starts_with("data:") { out.push_str(&css[i..close]); } else { out.push_str("none"); }
            i = close;
            continue;
        }
        let ch = css[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

impl Theme {
    /// Validate and normalize. `imported` themes get their CSS sanitized.
    pub fn validated(mut self, imported: bool) -> Result<Self, String> {
        if !safe_id(&self.id) { return Err(format!("invalid theme id {:?}", self.id)); }
        if self.name.trim().is_empty() { self.name = self.id.clone(); }
        self.name = self.name.chars().filter(|c| !c.is_control()).take(60).collect();
        if self.mode != "light" { self.mode = "dark".into(); }
        for (k, v) in &self.colors {
            if !safe_id(k) { return Err(format!("invalid color key {k:?}")); }
            if !safe_value(v) { return Err(format!("invalid value for color {k}: {v:?}")); }
        }
        for (what, v) in [("font_ui", &self.font_ui), ("font_mono", &self.font_mono)] {
            if !safe_value(v) { return Err(format!("invalid {what}")); }
        }
        if !self.start_background.is_empty() && !safe_value(&self.start_background) { return Err("invalid start_background".into()); }
        self.radius = self.radius.min(28);
        if !["compact", "comfortable", "spacious"].contains(&self.density.as_str()) { self.density = "comfortable".into(); }
        if imported {
            self.css = sanitize_imported_css(&self.css);
            self.start_background = sanitize_imported_css(&self.start_background);
            self.builtin = false;
        }
        Ok(self)
    }

    /// CSS custom properties for this theme, with missing colors filled from `fallback`.
    pub fn to_css(&self, fallback: Option<&Theme>) -> String {
        let mut css = String::from(":root{");
        css.push_str(&format!("color-scheme:{};", self.mode));
        for k in COLOR_KEYS {
            if let Some(v) = self.colors.get(*k).or_else(|| fallback.and_then(|f| f.colors.get(*k))) {
                css.push_str(&format!("--em-{k}:{v};"));
            }
        }
        for (k, v) in &self.colors {
            if !COLOR_KEYS.contains(&k.as_str()) { css.push_str(&format!("--em-{k}:{v};")); }
        }
        let (pad, row) = match self.density.as_str() { "compact" => (4, 28), "spacious" => (10, 40), _ => (7, 34) };
        css.push_str(&format!("--em-font:{};--em-mono:{};--em-radius:{}px;--em-radius-sm:{}px;--em-pad:{pad}px;--em-row:{row}px;", self.font_ui, self.font_mono, self.radius, (self.radius as f32 * 0.6).round()));
        if !self.start_background.is_empty() { css.push_str(&format!("--em-start-bg:{};", self.start_background)); }
        css.push('}');
        css.push_str(&self.css);
        css
    }
}

/// A theme pack: one portable JSON file holding any number of themes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemePack {
    pub emerald_theme_pack: u32,
    pub name: String,
    pub themes: Vec<Theme>,
}

/// Parse a theme pack or a single theme JSON. Returns validated themes.
pub fn parse_import(json: &str) -> Result<Vec<Theme>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not valid JSON: {e}"))?;
    let raw: Vec<Theme> = if v.get("emerald_theme_pack").is_some() {
        serde_json::from_value::<ThemePack>(v).map_err(|e| e.to_string())?.themes
    } else {
        vec![serde_json::from_value(v).map_err(|e| e.to_string())?]
    };
    if raw.is_empty() { return Err("the pack contains no themes".into()); }
    raw.into_iter().map(|t| t.validated(true)).collect()
}

pub fn export_pack(name: &str, themes: &[Theme]) -> String {
    let themes = themes.iter().cloned().map(|mut t| { t.builtin = false; t }).collect();
    serde_json::to_string_pretty(&ThemePack { emerald_theme_pack: 1, name: name.into(), themes }).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins_are_valid_and_complete() {
        let b = builtins();
        assert!(b.len() >= 5);
        for t in &b {
            let v = t.clone().validated(false).unwrap_or_else(|e| panic!("{}: {e}", t.id));
            for k in COLOR_KEYS { assert!(v.colors.contains_key(*k), "{} missing {k}", t.id); }
        }
        assert!(b.iter().any(|t| t.id == "emerald-dark") && b.iter().any(|t| t.id == "emerald-light"));
    }
    #[test]
    fn rejects_breakouts() {
        let mut t = Theme { id: "x".into(), ..Theme::default() };
        t.colors.insert("bg".into(), "red;} body{display:none".into());
        assert!(t.validated(true).is_err());
        assert!(Theme { id: "../../etc".into(), ..Theme::default() }.validated(true).is_err());
    }
    #[test]
    fn sanitizes_remote_urls() {
        let css = "@import url(https://evil.test/a.css); .a{background:url('https://evil.test/p.png')} .b{background:url(data:image/png;base64,AAA)}";
        let s = sanitize_imported_css(css);
        assert!(!s.contains("evil"));
        assert!(s.contains("data:image/png"));
    }
    #[test]
    fn pack_roundtrip() {
        let pack = export_pack("Mine", &builtins()[..2]);
        let back = parse_import(&pack).unwrap();
        assert_eq!(back.len(), 2);
        assert!(back.iter().all(|t| !t.builtin));
        let css = back[0].to_css(None);
        assert!(css.starts_with(":root{color-scheme:"));
        assert!(css.contains("--em-accent:"));
    }
}
