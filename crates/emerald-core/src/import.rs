//! Importing from Chrome and Edge profile folders, plus HTML/CSV files.
//!
//! What works where (stated honestly in the UI and FEATURES.md):
//! * Bookmarks and history: all platforms (Chromium's `Bookmarks` JSON and `History` SQLite).
//! * Passwords:
//!   - Windows Edge, and Chrome < 127: `v10` values (AES-256-GCM, key protected by DPAPI). ✔
//!   - Windows Chrome ≥ 127: `v20` "app-bound" encryption. The key can only be unwrapped by
//!     Chrome's own elevation service, which refuses other programs. ✘ → use Chrome's
//!     "Export passwords" CSV.
//!   - Linux `v10` (no keyring, fixed key). ✔ `v11` (GNOME Keyring / KWallet). ✘ → CSV.
//!   - macOS (Keychain-protected key). ✘ → CSV.
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub browser: String,
    pub name: String,
    pub dir: PathBuf,
    pub has_bookmarks: bool,
    pub has_history: bool,
    pub has_passwords: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BookmarkItem { pub title: String, pub url: String, pub folder: String, pub added: u64 }

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistoryItem { pub url: String, pub title: String, pub visits: i64, pub last_ms: i64 }

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PasswordItem { pub origin: String, pub username: String, pub password: String }

#[derive(Debug, Default, Serialize)]
pub struct PasswordResult { pub items: Vec<PasswordItem>, pub undecryptable: usize, pub reason: Option<String> }

/// Chromium user-data roots for Chrome and Edge on this OS.
pub fn browser_roots() -> Vec<(String, PathBuf)> {
    // Tests point this at a fixture so they never touch the user's real browser profiles.
    if let Ok(only) = std::env::var("EMERALD_IMPORT_ROOT") {
        return vec![("Test".into(), PathBuf::from(only))];
    }
    let mut v = vec![];
    #[cfg(windows)]
    if let Some(l) = dirs::data_local_dir() {
        v.push(("Chrome".into(), l.join("Google/Chrome/User Data")));
        v.push(("Edge".into(), l.join("Microsoft/Edge/User Data")));
        v.push(("Brave".into(), l.join("BraveSoftware/Brave-Browser/User Data")));
    }
    #[cfg(target_os = "linux")]
    if let Some(c) = dirs::config_dir() {
        v.push(("Chrome".into(), c.join("google-chrome")));
        v.push(("Chromium".into(), c.join("chromium")));
        v.push(("Edge".into(), c.join("microsoft-edge")));
        v.push(("Brave".into(), c.join("BraveSoftware/Brave-Browser")));
    }
    #[cfg(target_os = "macos")]
    if let Some(a) = dirs::data_dir() {
        v.push(("Chrome".into(), a.join("Google/Chrome")));
        v.push(("Edge".into(), a.join("Microsoft Edge")));
    }
    v
}

pub fn detect() -> Vec<Profile> {
    let mut out = vec![];
    for (browser, root) in browser_roots() {
        if !root.is_dir() { continue; }
        // Friendly names from Local State; fall back to folder names.
        let names: serde_json::Value = std::fs::read(root.join("Local State")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.join("Preferences").exists()).collect();
        dirs.sort();
        for dir in dirs {
            let folder = dir.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            let name = names["profile"]["info_cache"][&folder]["name"].as_str().map(String::from).unwrap_or(folder);
            out.push(Profile {
                browser: browser.clone(), name, has_bookmarks: dir.join("Bookmarks").exists(), has_history: dir.join("History").exists(),
                has_passwords: dir.join("Login Data").exists(), dir,
            });
        }
    }
    out
}

/// Parse Chromium's `Bookmarks` JSON.
pub fn bookmarks(profile_dir: &Path, folder_prefix: &str) -> anyhow::Result<Vec<BookmarkItem>> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(profile_dir.join("Bookmarks"))?)?;
    let mut out = vec![];
    fn walk(n: &serde_json::Value, path: &str, out: &mut Vec<BookmarkItem>) {
        match n["type"].as_str() {
            Some("url") => {
                let url = n["url"].as_str().unwrap_or("").to_string();
                if url.starts_with("http") || url.starts_with("file:") {
                    let added = n["date_added"].as_str().and_then(|d| d.parse::<i64>().ok()).map(webkit_to_unix_ms).unwrap_or(0).max(0) as u64;
                    out.push(BookmarkItem { title: n["name"].as_str().unwrap_or("").into(), url, folder: path.into(), added });
                }
            }
            Some("folder") => {
                let name = n["name"].as_str().unwrap_or("");
                let p = if path.is_empty() { name.to_string() } else { format!("{path}/{name}") };
                for c in n["children"].as_array().into_iter().flatten() { walk(c, &p, out); }
            }
            _ => {}
        }
    }
    for (key, label) in [("bookmark_bar", "Bookmarks bar"), ("other", "Other bookmarks"), ("synced", "Mobile bookmarks")] {
        let root = &v["roots"][key];
        for c in root["children"].as_array().into_iter().flatten() { walk(c, &format!("{folder_prefix}{label}"), &mut out); }
    }
    Ok(out)
}

/// WebKit epoch (µs since 1601-01-01) → Unix ms.
pub fn webkit_to_unix_ms(us: i64) -> i64 { us / 1000 - 11_644_473_600_000 }

/// Copy a (possibly locked) SQLite file to a temp location before reading it.
fn snapshot(src: &Path) -> anyhow::Result<PathBuf> {
    let mut tmp = std::env::temp_dir();
    let mut r = [0u8; 8];
    getrandom::getrandom(&mut r)?;
    tmp.push(format!("emerald-import-{}.db", u64::from_le_bytes(r)));
    std::fs::copy(src, &tmp).map_err(|e| anyhow::anyhow!("could not read {} ({e}). Close the browser and try again.", src.display()))?;
    Ok(tmp)
}

pub fn history(profile_dir: &Path, limit: usize) -> anyhow::Result<Vec<HistoryItem>> {
    let tmp = snapshot(&profile_dir.join("History"))?;
    let res = (|| -> anyhow::Result<Vec<HistoryItem>> {
        let db = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut st = db.prepare("SELECT url, title, visit_count, last_visit_time FROM urls WHERE hidden = 0 ORDER BY last_visit_time DESC LIMIT ?1")?;
        let rows = st.query_map([limit as i64], |r| Ok(HistoryItem { url: r.get(0)?, title: r.get(1)?, visits: r.get(2)?, last_ms: webkit_to_unix_ms(r.get(3)?) }))?;
        Ok(rows.flatten().filter(|h| h.url.starts_with("http")).collect())
    })();
    let _ = std::fs::remove_file(&tmp);
    res
}

/// Read and decrypt saved passwords.
pub fn passwords(profile_dir: &Path) -> anyhow::Result<PasswordResult> {
    let user_data = profile_dir.parent().unwrap_or(profile_dir);
    let key = os_crypt_key(user_data);
    let tmp = snapshot(&profile_dir.join("Login Data"))?;
    let rows: Vec<(String, String, Vec<u8>)> = (|| -> anyhow::Result<_> {
        let db = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut st = db.prepare("SELECT origin_url, username_value, password_value FROM logins WHERE blacklisted_by_user = 0")?;
        let r = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.flatten().collect();
        Ok(r)
    })().inspect_err(|_| { let _ = std::fs::remove_file(&tmp); })?;
    let _ = std::fs::remove_file(&tmp);
    let mut res = PasswordResult::default();
    let mut saw_v20 = false;
    for (url, user, blob) in rows {
        if blob.starts_with(b"v20") { saw_v20 = true; res.undecryptable += 1; continue; }
        match decrypt_value(&blob, key.as_deref()) {
            Some(pw) if !pw.is_empty() => {
                if let Some(origin) = crate::vault::origin_of(&url) { res.items.push(PasswordItem { origin, username: user, password: pw }); }
            }
            _ => res.undecryptable += 1,
        }
    }
    if res.undecryptable > 0 {
        res.reason = Some(if saw_v20 {
            "Chrome protects these with app-bound encryption, which only Chrome itself can unlock. In Chrome, open Settings → Passwords → Export passwords, then import the CSV here.".into()
        } else {
            "These are protected by your system keyring. Export them from the browser as CSV and import that file instead.".into()
        });
    }
    Ok(res)
}

/// The `os_crypt` key for Chromium `v10` values on this platform.
fn os_crypt_key(user_data: &Path) -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let ls: serde_json::Value = serde_json::from_slice(&std::fs::read(user_data.join("Local State")).ok()?).ok()?;
        let enc = STANDARD.decode(ls["os_crypt"]["encrypted_key"].as_str()?).ok()?;
        let enc = enc.strip_prefix(b"DPAPI")?;
        return crate::vault::unprotect(enc).ok();
    }
    #[cfg(not(windows))]
    {
        let _ = user_data;
        // Linux without a keyring: PBKDF2-HMAC-SHA1("peanuts", "saltysalt", 1 iteration, 16 bytes).
        let mut k = [0u8; 16];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(b"peanuts", b"saltysalt", 1, &mut k);
        Some(k.to_vec())
    }
}

pub fn decrypt_value(blob: &[u8], key: Option<&[u8]>) -> Option<String> {
    let key = key?;
    if let Some(rest) = blob.strip_prefix(b"v10") {
        if key.len() == 32 {
            // Windows: AES-256-GCM, 12-byte nonce, 16-byte tag at the end.
            use aes_gcm::aead::{Aead, KeyInit};
            if rest.len() < 12 + 16 { return None; }
            let c = aes_gcm::Aes256Gcm::new_from_slice(key).ok()?;
            let pt = c.decrypt(aes_gcm::Nonce::from_slice(&rest[..12]), &rest[12..]).ok()?;
            return String::from_utf8(pt).ok();
        }
        // Linux/macOS: AES-128-CBC, IV = 16 spaces, PKCS#7.
        use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
        let dec = cbc::Decryptor::<aes::Aes128>::new_from_slices(key, &[b' '; 16]).ok()?;
        let pt = dec.decrypt_padded_vec_mut::<Pkcs7>(rest).ok()?;
        // Newer Linux builds prefix the plaintext with a 32-byte SHA-256 of the host key.
        let s = String::from_utf8(pt.clone()).ok().or_else(|| String::from_utf8(pt.get(32..)?.to_vec()).ok())?;
        return Some(s);
    }
    None
}

// ------------------------------------------------------------------ file fallbacks

/// Parse a Netscape bookmark file (exported by every browser).
pub fn parse_bookmarks_html(html: &str) -> Vec<BookmarkItem> {
    let mut out = vec![];
    let mut stack: Vec<String> = vec![];
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    let mut pending_folder: Option<String> = None;
    while let Some(off) = lower[i..].find('<') {
        let start = i + off;
        let end = match lower[start..].find('>') { Some(e) => start + e + 1, None => break };
        let tag = &lower[start..end];
        if tag.starts_with("<h3") {
            let close = lower[end..].find("</h3>").map(|c| end + c).unwrap_or(end);
            pending_folder = Some(decode_entities(&html[end..close]));
            i = close;
            continue;
        } else if tag.starts_with("<dl") {
            stack.push(pending_folder.take().unwrap_or_default());
        } else if tag.starts_with("</dl") {
            stack.pop();
        } else if tag.starts_with("<a ") {
            let raw_tag = &html[start..end];
            let href = attr(raw_tag, "href").unwrap_or_default();
            let added = attr(raw_tag, "add_date").and_then(|d| d.parse::<u64>().ok()).map(|s| s * 1000).unwrap_or(0);
            let close = lower[end..].find("</a>").map(|c| end + c).unwrap_or(end);
            let title = decode_entities(html[end..close].trim());
            if href.starts_with("http") || href.starts_with("file:") {
                let folder = stack.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/");
                out.push(BookmarkItem { title, url: decode_entities(&href), folder, added });
            }
            i = close;
            continue;
        }
        i = end;
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let p = lower.find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[p..];
    let (q, rest) = match rest.chars().next()? { '"' => ('"', &rest[1..]), '\'' => ('\'', &rest[1..]), _ => (' ', rest) };
    Some(rest.split(q).next()?.trim_end_matches('>').to_string())
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'")
}

/// Parse a password CSV (Chrome, Edge, Firefox, Bitwarden and 1Password exports all use
/// a header row with some of: name,url,username,password,note / origin / login_uri …).
pub fn parse_passwords_csv(text: &str) -> Vec<PasswordItem> {
    let rows = parse_csv(text);
    let Some(header) = rows.first() else { return vec![] };
    let col = |names: &[&str]| header.iter().position(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)));
    let (Some(u), Some(user), Some(pw)) = (col(&["url", "origin", "login_uri", "website", "web site"]), col(&["username", "login_username", "user", "login"]), col(&["password", "login_password"])) else { return vec![] };
    rows.iter().skip(1).filter_map(|r| {
        let origin = crate::vault::origin_of(r.get(u)?)?;
        let password = r.get(pw)?.clone();
        (!password.is_empty()).then(|| PasswordItem { origin, username: r.get(user).cloned().unwrap_or_default(), password })
    }).collect()
}

/// RFC 4180 CSV.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut q = false;
    let mut it = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = it.next() {
        match (q, c) {
            (true, '"') if it.peek() == Some(&'"') => { field.push('"'); it.next(); }
            (true, '"') => q = false,
            (true, c) => field.push(c),
            (false, '"') => q = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => { row.push(std::mem::take(&mut field)); rows.push(std::mem::take(&mut row)); }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() { row.push(field); rows.push(row); }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_profile_roundtrip() {
        // Build a fake Chromium profile: Bookmarks JSON, History and Login Data SQLite.
        let root = std::env::temp_dir().join(format!("em-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let prof = root.join("Default");
        std::fs::create_dir_all(&prof).unwrap();
        std::fs::write(prof.join("Preferences"), "{}").unwrap();
        std::fs::write(root.join("Local State"), r#"{"profile":{"info_cache":{"Default":{"name":"Personal"}}}}"#).unwrap();
        std::fs::write(prof.join("Bookmarks"), r#"{"roots":{"bookmark_bar":{"children":[{"type":"url","name":"Rust","url":"https://www.rust-lang.org/","date_added":"13300000000000000"},{"type":"folder","name":"Dev","children":[{"type":"url","name":"Docs","url":"https://docs.rs/"},{"type":"url","name":"JS","url":"javascript:alert(1)"}]}]},"other":{"children":[]},"synced":{"children":[]}}}"#).unwrap();
        let h = rusqlite::Connection::open(prof.join("History")).unwrap();
        h.execute_batch("CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, typed_count INTEGER, last_visit_time INTEGER, hidden INTEGER); INSERT INTO urls VALUES(1,'https://example.com/','Example',3,0,13300000000000000,0),(2,'https://hidden.test/','H',1,0,13300000000000000,1);").unwrap();
        drop(h);
        // Linux-style v10 password encrypted with the well-known "peanuts" key.
        let mut k = [0u8; 16];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(b"peanuts", b"saltysalt", 1, &mut k);
        use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        let ct = cbc::Encryptor::<aes::Aes128>::new_from_slices(&k, &[b' '; 16]).unwrap().encrypt_padded_vec_mut::<Pkcs7>(b"hunter2");
        let mut blob = b"v10".to_vec();
        blob.extend(ct);
        let l = rusqlite::Connection::open(prof.join("Login Data")).unwrap();
        l.execute_batch("CREATE TABLE logins(origin_url TEXT, username_value TEXT, password_value BLOB, blacklisted_by_user INTEGER);").unwrap();
        l.execute("INSERT INTO logins VALUES('https://login.example.com/signin','ada',?1,0)", [&blob]).unwrap();
        l.execute("INSERT INTO logins VALUES('https://x.test/','bob',?1,0)", [b"v20garbage".to_vec()]).unwrap();
        drop(l);

        let b = bookmarks(&prof, "Chrome/").unwrap();
        assert_eq!(b.len(), 2, "javascript: bookmark skipped");
        assert_eq!(b[1].folder, "Chrome/Bookmarks bar/Dev");
        assert!(b[0].added > 1_600_000_000_000);
        let hi = history(&prof, 100).unwrap();
        assert_eq!(hi.len(), 1);
        assert_eq!(hi[0].visits, 3);
        #[cfg(not(windows))]
        {
            let p = passwords(&prof).unwrap();
            assert_eq!(p.items, vec![PasswordItem { origin: "https://login.example.com".into(), username: "ada".into(), password: "hunter2".into() }]);
            assert_eq!(p.undecryptable, 1);
            assert!(p.reason.unwrap().contains("app-bound"));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn netscape_html() {
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p>
          <DT><H3 ADD_DATE="1">Toolbar</H3>
          <DL><p><DT><A HREF="https://a.test/?x=1&amp;y=2" ADD_DATE="1700000000">A &amp; B</A>
            <DT><H3>Nested</H3><DL><p><DT><A HREF="https://b.test/">B</A></DL><p>
          </DL><p>
          <DT><A HREF="https://c.test/">C</A>
        </DL>"#;
        let b = parse_bookmarks_html(html);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], BookmarkItem { title: "A & B".into(), url: "https://a.test/?x=1&y=2".into(), folder: "Toolbar".into(), added: 1_700_000_000_000 });
        assert_eq!(b[1].folder, "Toolbar/Nested");
        assert_eq!(b[2].folder, "");
    }

    #[test]
    fn csv_passwords() {
        let csv = "name,url,username,password,note\r\nEx,https://ex.test/login,\"ada, l\",\"p\"\"w\",\nBad,not a url,u,p,\n";
        let p = parse_passwords_csv(csv);
        assert_eq!(p, vec![PasswordItem { origin: "https://ex.test".into(), username: "ada, l".into(), password: "p\"w".into() }]);
        let fx = "\u{feff}\"url\",\"username\",\"password\"\n\"https://f.test\",\"u\",\"pw\"\n";
        assert_eq!(parse_passwords_csv(fx).len(), 1);
    }
}
