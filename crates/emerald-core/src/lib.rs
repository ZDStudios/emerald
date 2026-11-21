//! Engine-independent core of Emerald.
//!
//! Nothing in this crate touches CEF, so all of it is unit-tested with plain `cargo test`.
pub mod crx;
pub mod fuzzy;
pub mod import;
pub mod vault;
pub mod paths;
pub mod session;
pub mod settings;
pub mod store;
pub mod theme;
pub mod url;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
