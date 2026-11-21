//! Tiny JSON persistence helpers shared by settings, session, notes, etc.
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

/// Load `T` from a JSON file. A missing file yields `T::default()`. A corrupt file is moved
/// aside to `<name>.corrupt` (never silently deleted) and the default is used.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("emerald: {} is corrupt ({e}); keeping a copy and starting fresh", path.display());
                let _ = std::fs::rename(path, path.with_extension("corrupt"));
                T::default()
            }
        },
        Err(_) => T::default(),
    }
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    crate::paths::write_atomic(path, &bytes)?;
    Ok(())
}
