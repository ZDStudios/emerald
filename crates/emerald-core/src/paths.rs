//! Where Emerald keeps its data.
//!
//! ```text
//! <root>/                  %LOCALAPPDATA%\Emerald | $XDG_DATA_HOME/emerald | ~/Library/Application Support/Emerald
//!   engine/                CEF root cache path (Chromium profile: cookies, extensions, cache)
//!   emerald/               our own state (settings.json, session.json, notes.json, history.db, …)
//!   emerald/themes/        user themes (*.json)
//! ```
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    /// `override_root` comes from `--profile=<dir>` (tests, benchmarks, portable mode).
    pub fn new(override_root: Option<PathBuf>) -> Self {
        let root = override_root.unwrap_or_else(|| {
            dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join(if cfg!(target_os = "linux") { "emerald" } else { "Emerald" })
        });
        let p = Self { root };
        for d in [p.engine(), p.data(), p.themes()] {
            let _ = std::fs::create_dir_all(d);
        }
        p
    }
    pub fn engine(&self) -> PathBuf { self.root.join("engine") }
    pub fn engine_profile(&self) -> PathBuf { self.engine().join("Default") }
    pub fn data(&self) -> PathBuf { self.root.join("emerald") }
    pub fn themes(&self) -> PathBuf { self.data().join("themes") }
    pub fn file(&self, name: &str) -> PathBuf { self.data().join(name) }
}

/// Write a file atomically (write to a temp sibling, then rename) so a crash never leaves
/// a half-written settings or session file behind.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp~");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
