//! Bookmarks (bookmarks.json): flat list with an optional folder path ("Imported/Chrome/Dev").
use emerald_core::paths::Paths;
use emerald_core::{now_ms, store};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Bookmark { pub url: String, pub title: String, pub folder: String, pub added: u64 }

pub fn load(p: &Paths) -> Vec<Bookmark> { store::load_json(&p.file("bookmarks.json")) }

pub fn save(p: &Paths, v: &[Bookmark]) { let _ = store::save_json(&p.file("bookmarks.json"), &v); }

pub fn add(p: &Paths, url: &str, title: &str, folder: &str) {
    if url.is_empty() { return; }
    let mut v = load(p);
    if v.iter().any(|b| b.url == url && b.folder == folder) { return; }
    v.push(Bookmark { url: url.into(), title: if title.is_empty() { url.into() } else { title.into() }, folder: folder.into(), added: now_ms() });
    save(p, &v);
}

/// Add many, skipping duplicates. Returns how many were new.
pub fn add_many(p: &Paths, items: Vec<Bookmark>) -> usize {
    let mut v = load(p);
    let mut n = 0;
    for b in items {
        if b.url.is_empty() || v.iter().any(|x| x.url == b.url && x.folder == b.folder) { continue; }
        v.push(b);
        n += 1;
    }
    save(p, &v);
    n
}

pub fn delete(p: &Paths, url: &str) {
    let mut v = load(p);
    v.retain(|b| b.url != url);
    save(p, &v);
}
