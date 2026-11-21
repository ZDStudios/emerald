//! The password vault: credentials encrypted at rest with AES-256-GCM.
//!
//! The 256-bit vault key is generated once and protected by the OS where possible:
//! * Windows: DPAPI (`CryptProtectData`), bound to the user account.
//! * Linux/macOS: stored in a 0600 key file. This only protects against other users,
//!   not against software running as you. That limitation is stated in FEATURES.md.
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Credential {
    pub id: u64,
    /// Scheme + host (+ port), e.g. `https://accounts.example.com`.
    pub origin: String,
    pub username: String,
    pub password: String,
    pub note: String,
    pub created: u64,
    pub used: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct Sealed { v: u32, nonce: String, data: String }

pub struct Vault {
    path: PathBuf,
    key: [u8; 32],
    pub items: Vec<Credential>,
}

pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "https" && scheme != "http" { return None; }
    let auth = rest.split(['/', '?', '#']).next()?;
    let auth = auth.rsplit_once('@').map(|x| x.1).unwrap_or(auth).to_ascii_lowercase();
    (!auth.is_empty()).then(|| format!("{scheme}://{auth}"))
}

impl Vault {
    /// Open (or create) the vault stored at `dir/vault.json` with its key next to it.
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        let key = load_or_create_key(&dir.join("vault.key"))?;
        let path = dir.join("vault.json");
        let items = match std::fs::read(&path) {
            Ok(b) => {
                let sealed: Sealed = serde_json::from_slice(&b)?;
                let plain = Aes256Gcm::new_from_slice(&key)?.decrypt(Nonce::from_slice(&B64.decode(sealed.nonce)?), B64.decode(sealed.data)?.as_ref())
                    .map_err(|_| anyhow::anyhow!("vault could not be decrypted (key mismatch)"))?;
                serde_json::from_slice(&plain)?
            }
            Err(_) => vec![],
        };
        Ok(Self { path, key, items })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce)?;
        let plain = serde_json::to_vec(&self.items)?;
        let data = Aes256Gcm::new_from_slice(&self.key)?.encrypt(Nonce::from_slice(&nonce), plain.as_ref()).map_err(|_| anyhow::anyhow!("encrypt"))?;
        let sealed = Sealed { v: 1, nonce: B64.encode(nonce), data: B64.encode(data) };
        crate::paths::write_atomic(&self.path, &serde_json::to_vec(&sealed)?)?;
        Ok(())
    }

    /// Insert or update (same origin + username). Returns true if something changed.
    pub fn upsert(&mut self, origin: &str, username: &str, password: &str, note: &str) -> bool {
        if origin.is_empty() || password.is_empty() { return false; }
        if let Some(c) = self.items.iter_mut().find(|c| c.origin == origin && c.username == username) {
            if c.password == password { return false; }
            c.password = password.into();
            return true;
        }
        let id = self.items.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        self.items.push(Credential { id, origin: origin.into(), username: username.into(), password: password.into(), note: note.into(), created: crate::now_ms(), used: 0 });
        true
    }

    pub fn for_origin(&self, origin: &str) -> Vec<&Credential> {
        self.items.iter().filter(|c| c.origin == origin).collect()
    }

    pub fn remove(&mut self, id: u64) { self.items.retain(|c| c.id != id); }
}

fn load_or_create_key(path: &Path) -> anyhow::Result<[u8; 32]> {
    if let Ok(b) = std::fs::read(path) {
        let raw = unprotect(&b)?;
        return raw.try_into().map_err(|_| anyhow::anyhow!("bad vault key length"));
    }
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k)?;
    let blob = protect(&k)?;
    std::fs::write(path, &blob)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(k)
}

#[cfg(windows)]
pub fn protect(data: &[u8]) -> anyhow::Result<Vec<u8>> { dpapi(data, true) }
#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> anyhow::Result<Vec<u8>> { dpapi(data, false) }

#[cfg(windows)]
fn dpapi(data: &[u8], encrypt: bool) -> anyhow::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        if encrypt { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) }
        else { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) }
    };
    if ok == 0 { anyhow::bail!("DPAPI call failed"); }
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData as _); }
    Ok(v)
}

#[cfg(not(windows))]
pub fn protect(data: &[u8]) -> anyhow::Result<Vec<u8>> { Ok(data.to_vec()) }
#[cfg(not(windows))]
pub fn unprotect(data: &[u8]) -> anyhow::Result<Vec<u8>> { Ok(data.to_vec()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_tamper() {
        let dir = std::env::temp_dir().join(format!("em-vault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut v = Vault::open(&dir).unwrap();
        assert!(v.upsert("https://example.com", "ada", "s3cret", ""));
        assert!(!v.upsert("https://example.com", "ada", "s3cret", ""), "no-op update");
        v.save().unwrap();
        let raw = std::fs::read_to_string(dir.join("vault.json")).unwrap();
        assert!(!raw.contains("s3cret") && !raw.contains("ada"), "encrypted at rest");
        let v2 = Vault::open(&dir).unwrap();
        assert_eq!(v2.for_origin("https://example.com")[0].password, "s3cret");
        // Tampering is detected (GCM tag).
        let mut sealed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let mut data = B64.decode(sealed["data"].as_str().unwrap()).unwrap();
        data[0] ^= 1;
        sealed["data"] = B64.encode(data).into();
        std::fs::write(dir.join("vault.json"), sealed.to_string()).unwrap();
        assert!(Vault::open(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn origins() {
        assert_eq!(origin_of("https://User@Accounts.Example.com:8443/login?x").as_deref(), Some("https://accounts.example.com:8443"));
        assert_eq!(origin_of("chrome://settings"), None);
    }
}
