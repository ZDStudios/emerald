//! Import glue: Chrome/Edge profiles and HTML/CSV files → Emerald bookmarks, history, vault.
use crate::bookmarks::{self, Bookmark};
use crate::shell::Shell;
use anyhow::{anyhow, Result};
use emerald_core::import as imp;
use serde_json::{json, Value};

pub fn detect() -> Value {
    json!(imp::detect().into_iter().map(|p| json!({
        "browser": p.browser, "name": p.name, "dir": p.dir.to_string_lossy(),
        "bookmarks": p.has_bookmarks, "history": p.has_history, "passwords": p.has_passwords,
    })).collect::<Vec<_>>())
}

/// `{dir, browser, bookmarks: bool, history: bool, passwords: bool}`
pub fn run(s: &mut Shell, a: &Value) -> Result<Value> {
    let dir = std::path::PathBuf::from(a["dir"].as_str().ok_or_else(|| anyhow!("missing dir"))?);
    // Only import from a detected browser profile folder.
    if !imp::detect().iter().any(|p| p.dir == dir) { return Err(anyhow!("unknown profile")); }
    let browser = a["browser"].as_str().unwrap_or("Browser");
    let mut report = json!({});
    if a["bookmarks"].as_bool().unwrap_or(false) {
        report["bookmarks"] = match imp::bookmarks(&dir, &format!("Imported from {browser}/")) {
            Ok(items) => json!({"added": bookmarks::add_many(&s.paths, items.into_iter().map(|b| Bookmark { url: b.url, title: b.title, folder: b.folder, added: b.added }).collect()) }),
            Err(e) => json!({"error": e.to_string()}),
        };
    }
    if a["history"].as_bool().unwrap_or(false) {
        report["history"] = match (imp::history(&dir, 50_000), s.history.as_mut()) {
            (Ok(items), Some(h)) => {
                h.begin();
                let n = items.iter().filter(|i| h.import(&i.url, &i.title, i.visits, i.last_ms)).count();
                h.commit();
                json!({"added": n})
            }
            (Err(e), _) => json!({"error": e.to_string()}),
            (_, None) => json!({"error": "history database unavailable"}),
        };
    }
    if a["passwords"].as_bool().unwrap_or(false) {
        report["passwords"] = match imp::passwords(&dir) {
            Ok(r) => {
                let added = crate::passwords::import(s, r.items.iter().map(|p| (p.origin.clone(), p.username.clone(), p.password.clone())));
                json!({"added": added, "skipped": r.undecryptable, "reason": r.reason})
            }
            Err(e) => json!({"error": e.to_string()}),
        };
    }
    s.push_state();
    Ok(report)
}

/// `{kind: "bookmarks-html" | "passwords-csv", text}`: file contents read by the page.
pub fn run_file(s: &mut Shell, a: &Value) -> Result<Value> {
    let text = a["text"].as_str().unwrap_or("");
    match a["kind"].as_str().unwrap_or("") {
        "bookmarks-html" => {
            let items = imp::parse_bookmarks_html(text);
            let n = bookmarks::add_many(&s.paths, items.into_iter().map(|b| Bookmark { url: b.url, title: b.title, folder: if b.folder.is_empty() { "Imported".into() } else { format!("Imported/{}", b.folder) }, added: b.added }).collect());
            Ok(json!({"bookmarks": {"added": n}}))
        }
        "passwords-csv" => {
            let items = imp::parse_passwords_csv(text);
            if items.is_empty() { return Err(anyhow!("No passwords found. Is this a password export CSV with url, username and password columns?")); }
            let n = crate::passwords::import(s, items.into_iter().map(|p| (p.origin, p.username, p.password)));
            Ok(json!({"passwords": {"added": n}}))
        }
        _ => Err(anyhow!("unknown file kind")),
    }
}
