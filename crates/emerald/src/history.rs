//! Browsing history (our own SQLite file, so it can be searched by the command bar,
//! imported into, and cleared independently of Chromium's internal history).
use rusqlite::{params, Connection};
use serde::Serialize;

pub struct History { db: Connection }

#[derive(Debug, Serialize, Clone)]
pub struct Entry { pub url: String, pub title: String, pub visits: i64, pub last: i64 }

fn recordable(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) && url.len() < 4096
}

impl History {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS urls(id INTEGER PRIMARY KEY, url TEXT UNIQUE NOT NULL, title TEXT NOT NULL DEFAULT '', visits INTEGER NOT NULL DEFAULT 0, last INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS visits(id INTEGER PRIMARY KEY, url_id INTEGER NOT NULL, ts INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS visits_ts ON visits(ts);
             CREATE INDEX IF NOT EXISTS urls_last ON urls(last);",
        )?;
        Ok(Self { db })
    }

    pub fn record(&mut self, url: &str, title: &str, ts: i64) {
        if !recordable(url) { return; }
        let r = (|| -> rusqlite::Result<()> {
            self.db.execute("INSERT INTO urls(url,title,visits,last) VALUES(?1,?2,1,?3) ON CONFLICT(url) DO UPDATE SET visits=visits+1, last=?3, title=CASE WHEN ?2<>'' THEN ?2 ELSE title END", params![url, title, ts])?;
            let id: i64 = self.db.query_row("SELECT id FROM urls WHERE url=?1", [url], |r| r.get(0))?;
            self.db.execute("INSERT INTO visits(url_id, ts) VALUES(?1, ?2)", params![id, ts])?;
            Ok(())
        })();
        if let Err(e) = r { eprintln!("emerald: history record: {e}"); }
    }

    pub fn set_title(&mut self, url: &str, title: &str) {
        if recordable(url) && !title.is_empty() {
            let _ = self.db.execute("UPDATE urls SET title=?2 WHERE url=?1", params![url, title]);
        }
    }

    /// Import an entry (from Chrome/Edge) without double counting on re-import.
    pub fn import(&mut self, url: &str, title: &str, visits: i64, last: i64) -> bool {
        if !recordable(url) { return false; }
        self.db.execute("INSERT INTO urls(url,title,visits,last) VALUES(?1,?2,?3,?4) ON CONFLICT(url) DO UPDATE SET visits=MAX(visits,?3), last=MAX(last,?4), title=CASE WHEN title='' THEN ?2 ELSE title END", params![url, title, visits.max(1), last]).is_ok()
    }

    pub fn begin(&self) { let _ = self.db.execute_batch("BEGIN"); }
    pub fn commit(&self) { let _ = self.db.execute_batch("COMMIT"); }

    /// Frecency-ordered search over URL and title.
    pub fn search(&self, q: &str, limit: usize) -> Vec<Entry> {
        let words: Vec<String> = q.split_whitespace().take(6).map(|w| format!("%{}%", w.replace(['%', '_'], ""))).collect();
        let mut sql = String::from("SELECT url,title,visits,last FROM urls WHERE 1=1");
        for i in 0..words.len() { sql.push_str(&format!(" AND (url LIKE ?{0} OR title LIKE ?{0})", i + 1)); }
        sql.push_str(&format!(" ORDER BY (visits * 1.0) / (1 + (strftime('%s','now')*1000 - last) / 86400000.0) DESC LIMIT {limit}"));
        let Ok(mut st) = self.db.prepare(&sql) else { return vec![] };
        let rows = st.query_map(rusqlite::params_from_iter(words.iter()), |r| Ok(Entry { url: r.get(0)?, title: r.get(1)?, visits: r.get(2)?, last: r.get(3)? }));
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    /// Visits, newest first, optionally filtered.
    pub fn recent(&self, q: &str, before: i64, limit: usize) -> Vec<Entry> {
        let like = format!("%{}%", q.replace(['%', '_'], ""));
        let Ok(mut st) = self.db.prepare("SELECT u.url,u.title,u.visits,v.ts FROM visits v JOIN urls u ON u.id=v.url_id WHERE v.ts < ?1 AND (u.url LIKE ?2 OR u.title LIKE ?2) ORDER BY v.ts DESC LIMIT ?3") else { return vec![] };
        let rows = st.query_map(params![before, like, limit as i64], |r| Ok(Entry { url: r.get(0)?, title: r.get(1)?, visits: r.get(2)?, last: r.get(3)? }));
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    pub fn delete_url(&mut self, url: &str) {
        let _ = self.db.execute("DELETE FROM visits WHERE url_id IN (SELECT id FROM urls WHERE url=?1)", [url]);
        let _ = self.db.execute("DELETE FROM urls WHERE url=?1", [url]);
    }

    pub fn clear_since(&mut self, since: i64) {
        let _ = self.db.execute("DELETE FROM visits WHERE ts >= ?1", [since]);
        let _ = self.db.execute("DELETE FROM urls WHERE id NOT IN (SELECT DISTINCT url_id FROM visits) AND last >= ?1", [since]);
    }

    pub fn count(&self) -> i64 { self.db.query_row("SELECT COUNT(*) FROM urls", [], |r| r.get(0)).unwrap_or(0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_search_delete() {
        let dir = std::env::temp_dir().join(format!("em-hist-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut h = History::open(&dir.join("h.db")).unwrap();
        h.record("https://docs.rs/serde", "serde - Rust", 1000);
        h.record("https://docs.rs/serde", "", 2000);
        h.record("emerald://settings/", "Settings", 3000);
        h.record("https://example.com/", "Example", 1500);
        assert_eq!(h.count(), 2);
        let r = h.search("serde rust", 10);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].visits, 2);
        assert_eq!(r[0].title, "serde - Rust");
        assert_eq!(h.recent("", i64::MAX, 10).len(), 3);
        h.delete_url("https://docs.rs/serde");
        assert!(h.search("serde", 10).is_empty());
        assert!(h.import("https://a.test/", "A", 5, 10));
        let _ = std::fs::remove_dir_all(dir);
    }
}
