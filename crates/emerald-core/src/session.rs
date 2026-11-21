//! The tab model: workspaces (spaces) → pinned tabs, folders/groups, loose tabs; plus parked
//! tabs and recently closed tabs. Persisted as `session.json`.
//!
//! Ordering uses fractional `order` keys within a *container*
//! (pinned section, top level of a workspace, or one folder), so a drag-and-drop move only
//! touches the moved item.
use serde::{Deserialize, Serialize};

pub type Id = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Tab {
    pub id: Id,
    pub workspace: Id,
    pub folder: Option<Id>,
    pub pinned: bool,
    pub order: f64,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub last_active: u64,
    pub opener: Option<Id>,
    pub muted: bool,
    pub zoom: f64,
}
impl Default for Tab {
    fn default() -> Self {
        Self { id: 0, workspace: 0, folder: None, pinned: false, order: 0.0, url: String::new(), title: String::new(), favicon: None, last_active: 0, opener: None, muted: false, zoom: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Folder {
    pub id: Id,
    pub workspace: Id,
    pub name: String,
    pub color: String,
    pub collapsed: bool,
    /// A "group" is a lightweight, colored tab group; a folder is a persistent container.
    /// They behave the same; the UI styles them differently.
    pub group: bool,
    pub order: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Split {
    pub left: Id,
    pub right: Id,
    /// Fraction of the content width given to `left` (0.2–0.8).
    pub ratio: f64,
}
impl Default for Split { fn default() -> Self { Self { left: 0, right: 0, ratio: 0.5 } } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Workspace {
    pub id: Id,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub active_tab: Option<Id>,
    pub split: Option<Split>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Parked {
    pub id: Id,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub parked_at: u64,
    /// When to gently remind (ms since epoch). `None` = no reminder.
    pub remind_at: Option<u64>,
    pub reminded: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Session {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: Id,
    pub tabs: Vec<Tab>,
    pub folders: Vec<Folder>,
    pub parked: Vec<Parked>,
    pub closed: Vec<Tab>,
    pub next_id: Id,
}

impl Default for Session {
    fn default() -> Self {
        let mut s = Self { workspaces: vec![], active_workspace: 0, tabs: vec![], folders: vec![], parked: vec![], closed: vec![], next_id: 1 };
        let id = s.add_workspace("Home", "🌿", "#2fb67c");
        s.active_workspace = id;
        s
    }
}

/// Where a tab or folder should go in a drag-and-drop move.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Target {
    pub workspace: Option<Id>,
    pub folder: Option<Id>,
    pub pinned: bool,
    /// Insert before this item (tab or folder id) in the destination container; `None` = append.
    pub before: Option<Id>,
}

/// One row of the sidebar, in display order.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Row {
    Tab { id: Id, depth: u32 },
    Folder { id: Id },
}

const MAX_CLOSED: usize = 25;

impl Session {
    fn alloc(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Repair invariants after loading a file written by an older/crashed build.
    pub fn normalize(&mut self) {
        if self.workspaces.is_empty() {
            *self = Self { tabs: std::mem::take(&mut self.tabs), parked: std::mem::take(&mut self.parked), ..Self::default() };
        }
        let max = self.tabs.iter().map(|t| t.id).chain(self.folders.iter().map(|f| f.id)).chain(self.workspaces.iter().map(|w| w.id)).chain(self.parked.iter().map(|p| p.id)).max().unwrap_or(0);
        self.next_id = self.next_id.max(max + 1);
        let ws: Vec<Id> = self.workspaces.iter().map(|w| w.id).collect();
        let first = ws[0];
        if !ws.contains(&self.active_workspace) { self.active_workspace = first; }
        let folders: Vec<(Id, Id)> = self.folders.iter().map(|f| (f.id, f.workspace)).collect();
        for t in &mut self.tabs {
            if !ws.contains(&t.workspace) { t.workspace = first; }
            if let Some(f) = t.folder {
                if !folders.iter().any(|(id, w)| *id == f && *w == t.workspace) { t.folder = None; }
            }
            if t.pinned { t.folder = None; }
        }
        self.folders.retain(|f| ws.contains(&f.workspace));
        let ids: Vec<Id> = self.tabs.iter().map(|t| t.id).collect();
        for w in &mut self.workspaces {
            if w.active_tab.is_some_and(|a| !ids.contains(&a)) { w.active_tab = None; }
            if w.split.as_ref().is_some_and(|s| !ids.contains(&s.left) || !ids.contains(&s.right)) { w.split = None; }
        }
    }

    // ---------- workspaces ----------
    pub fn add_workspace(&mut self, name: &str, icon: &str, color: &str) -> Id {
        let id = self.alloc();
        self.workspaces.push(Workspace { id, name: name.into(), icon: icon.into(), color: color.into(), active_tab: None, split: None });
        id
    }
    pub fn workspace(&self, id: Id) -> Option<&Workspace> { self.workspaces.iter().find(|w| w.id == id) }
    pub fn workspace_mut(&mut self, id: Id) -> Option<&mut Workspace> { self.workspaces.iter_mut().find(|w| w.id == id) }
    pub fn active(&self) -> &Workspace { self.workspace(self.active_workspace).unwrap_or(&self.workspaces[0]) }
    /// Delete a workspace; its tabs are closed (returned so the caller can tear down browsers).
    pub fn remove_workspace(&mut self, id: Id) -> Vec<Id> {
        if self.workspaces.len() <= 1 { return vec![]; }
        let closing: Vec<Id> = self.tabs.iter().filter(|t| t.workspace == id).map(|t| t.id).collect();
        for t in &closing { self.close_tab(*t); }
        self.folders.retain(|f| f.workspace != id);
        self.workspaces.retain(|w| w.id != id);
        if self.active_workspace == id { self.active_workspace = self.workspaces[0].id; }
        closing
    }

    // ---------- tabs ----------
    pub fn tab(&self, id: Id) -> Option<&Tab> { self.tabs.iter().find(|t| t.id == id) }
    pub fn tab_mut(&mut self, id: Id) -> Option<&mut Tab> { self.tabs.iter_mut().find(|t| t.id == id) }
    pub fn active_tab(&self) -> Option<Id> { self.active().active_tab }
    pub fn tabs_in(&self, ws: Id) -> impl Iterator<Item = &Tab> { self.tabs.iter().filter(move |t| t.workspace == ws) }

    /// Open a new tab. With `after`, it is placed right below that tab, in the same container.
    pub fn new_tab(&mut self, url: &str, after: Option<Id>, activate: bool) -> Id {
        let id = self.alloc();
        let ws = self.active_workspace;
        let (folder, pinned, order) = match after.and_then(|a| self.tab(a)).filter(|a| a.workspace == ws) {
            // Children of a pinned tab go to the top of the loose list, not into pinned.
            Some(a) if !a.pinned => {
                let (f, o) = (a.folder, a.order);
                let next = self.container_orders(ws, f, false).into_iter().filter(|x| *x > o).fold(f64::INFINITY, f64::min);
                (f, false, if next.is_finite() { (o + next) / 2.0 } else { o + 1.0 })
            }
            _ => (None, false, self.container_orders(ws, None, false).into_iter().fold(0.0, f64::max) + 1.0),
        };
        self.tabs.push(Tab { id, workspace: ws, folder, pinned, order, url: url.into(), title: String::new(), last_active: crate::now_ms(), opener: after, ..Tab::default() });
        if activate { self.activate(id); }
        id
    }

    pub fn activate(&mut self, id: Id) {
        let Some(t) = self.tab_mut(id) else { return };
        t.last_active = crate::now_ms();
        let ws = t.workspace;
        self.active_workspace = ws;
        if let Some(w) = self.workspace_mut(ws) {
            // Leaving a split by activating a third tab collapses the split.
            if w.split.as_ref().is_some_and(|s| s.left != id && s.right != id) { w.split = None; }
            w.active_tab = Some(id);
        }
    }

    /// Close a tab and return the tab that should become active (if the closed tab was active).
    pub fn close_tab(&mut self, id: Id) -> Option<Id> {
        let pos = self.tabs.iter().position(|t| t.id == id)?;
        let order = self.display_order(self.tabs[pos].workspace);
        let tab = self.tabs.remove(pos);
        let was_active = self.workspace(tab.workspace).and_then(|w| w.active_tab) == Some(id);
        let mut next = None;
        if was_active {
            // Prefer the opener (if still open), then the tab below, then the tab above.
            next = tab.opener.filter(|o| self.tab(*o).is_some_and(|t| t.workspace == tab.workspace));
            if next.is_none() {
                let i = order.iter().position(|x| *x == id).unwrap_or(0);
                next = order.get(i + 1).copied().or_else(|| i.checked_sub(1).and_then(|j| order.get(j).copied()));
            }
        }
        if let Some(w) = self.workspace_mut(tab.workspace) {
            if w.split.as_ref().is_some_and(|s| s.left == id || s.right == id) { w.split = None; }
            if was_active { w.active_tab = next; }
        }
        for t in &mut self.tabs { if t.opener == Some(id) { t.opener = tab.opener; } }
        if !tab.url.is_empty() && !tab.url.starts_with("emerald://start") {
            self.closed.insert(0, tab);
            self.closed.truncate(MAX_CLOSED);
        }
        next
    }

    pub fn reopen_closed(&mut self) -> Option<Id> {
        let mut t = (!self.closed.is_empty()).then(|| self.closed.remove(0))?;
        let id = self.alloc();
        t.id = id;
        if self.workspace(t.workspace).is_none() { t.workspace = self.active_workspace; }
        if t.folder.is_some_and(|f| !self.folders.iter().any(|x| x.id == f)) { t.folder = None; }
        t.opener = None;
        self.tabs.push(t);
        self.activate(id);
        Some(id)
    }

    // ---------- folders / groups ----------
    pub fn add_folder(&mut self, name: &str, group: bool, color: &str) -> Id {
        let id = self.alloc();
        let ws = self.active_workspace;
        let order = self.container_orders(ws, None, false).into_iter().fold(0.0, f64::max) + 1.0;
        self.folders.push(Folder { id, workspace: ws, name: name.into(), color: color.into(), collapsed: false, group, order });
        id
    }
    pub fn folder_mut(&mut self, id: Id) -> Option<&mut Folder> { self.folders.iter_mut().find(|f| f.id == id) }
    /// Remove a folder; with `close_tabs` its tabs are closed (ids returned), otherwise they move out in place.
    pub fn remove_folder(&mut self, id: Id, close_tabs: bool) -> Vec<Id> {
        let Some(f) = self.folders.iter().find(|f| f.id == id).cloned() else { return vec![] };
        let mut kids: Vec<&Tab> = self.tabs.iter().filter(|t| t.folder == Some(id)).collect();
        kids.sort_by(|a, b| a.order.total_cmp(&b.order));
        let inside: Vec<Id> = kids.into_iter().map(|t| t.id).collect();
        if close_tabs {
            for t in &inside { self.close_tab(*t); }
        } else {
            let n = inside.len().max(1) as f64;
            for (i, tid) in inside.iter().enumerate() {
                if let Some(t) = self.tab_mut(*tid) { t.folder = None; t.order = f.order + (i as f64 + 1.0) / (n + 1.0) * 0.999; }
            }
        }
        self.folders.retain(|x| x.id != id);
        if close_tabs { inside } else { vec![] }
    }

    fn container_orders(&self, ws: Id, folder: Option<Id>, pinned: bool) -> Vec<f64> {
        let mut v: Vec<f64> = self.tabs.iter().filter(|t| t.workspace == ws && t.folder == folder && t.pinned == pinned).map(|t| t.order).collect();
        if folder.is_none() && !pinned {
            v.extend(self.folders.iter().filter(|f| f.workspace == ws).map(|f| f.order));
        }
        v
    }

    /// Move a tab or a folder (`id` may be either). Folders can only move within the top level.
    pub fn move_item(&mut self, id: Id, target: &Target) -> bool {
        let is_folder = self.folders.iter().any(|f| f.id == id);
        let ws = target.workspace.unwrap_or(self.active_workspace);
        if self.workspace(ws).is_none() { return false; }
        let (folder, pinned) = if is_folder { (None, false) } else if target.pinned { (None, true) } else { (target.folder, false) };
        if folder.is_some_and(|f| !self.folders.iter().any(|x| x.id == f && x.workspace == ws)) { return false; }
        // Siblings in destination (excluding the moved item), sorted.
        let mut sib: Vec<(Id, f64)> = self.tabs.iter().filter(|t| t.id != id && t.workspace == ws && t.folder == folder && t.pinned == pinned).map(|t| (t.id, t.order)).collect();
        if folder.is_none() && !pinned { sib.extend(self.folders.iter().filter(|f| f.id != id && f.workspace == ws).map(|f| (f.id, f.order))); }
        sib.sort_by(|a, b| a.1.total_cmp(&b.1));
        let idx = target.before.and_then(|b| sib.iter().position(|(i, _)| *i == b)).unwrap_or(sib.len());
        let prev = if idx == 0 { None } else { Some(sib[idx - 1].1) };
        let next = sib.get(idx).map(|x| x.1);
        let order = match (prev, next) {
            (None, None) => 1.0,
            (Some(p), None) => p + 1.0,
            (None, Some(n)) => n - 1.0,
            (Some(p), Some(n)) => (p + n) / 2.0,
        };
        if is_folder {
            let old_ws = self.folders.iter().find(|f| f.id == id).map(|f| f.workspace);
            if let Some(f) = self.folder_mut(id) { f.order = order; f.workspace = ws; }
            if old_ws != Some(ws) { for t in &mut self.tabs { if t.folder == Some(id) { t.workspace = ws; } } }
        } else {
            let old_ws = self.tab(id).map(|t| t.workspace);
            let Some(t) = self.tab_mut(id) else { return false };
            t.workspace = ws; t.folder = folder; t.pinned = pinned; t.order = order;
            if old_ws != Some(ws) {
                if let Some(w) = old_ws.and_then(|o| self.workspace_mut(o)) {
                    if w.active_tab == Some(id) { w.active_tab = None; }
                    if w.split.as_ref().is_some_and(|s| s.left == id || s.right == id) { w.split = None; }
                }
            }
        }
        self.renormalize_if_needed(ws, folder, pinned);
        true
    }

    fn renormalize_if_needed(&mut self, ws: Id, folder: Option<Id>, pinned: bool) {
        let mut o = self.container_orders(ws, folder, pinned);
        o.sort_by(f64::total_cmp);
        if !o.windows(2).any(|w| (w[1] - w[0]).abs() < 1e-9) { return; }
        let mut items: Vec<(bool, Id, f64)> = self.tabs.iter().filter(|t| t.workspace == ws && t.folder == folder && t.pinned == pinned).map(|t| (false, t.id, t.order)).collect();
        if folder.is_none() && !pinned { items.extend(self.folders.iter().filter(|f| f.workspace == ws).map(|f| (true, f.id, f.order))); }
        items.sort_by(|a, b| a.2.total_cmp(&b.2));
        for (i, (is_f, id, _)) in items.into_iter().enumerate() {
            if is_f { if let Some(f) = self.folder_mut(id) { f.order = i as f64 + 1.0; } } else if let Some(t) = self.tab_mut(id) { t.order = i as f64 + 1.0; }
        }
    }

    /// Sidebar rows for a workspace (pinned tabs are rendered separately by the UI).
    pub fn rows(&self, ws: Id) -> Vec<Row> {
        let mut top: Vec<(f64, Row)> = self.tabs.iter().filter(|t| t.workspace == ws && !t.pinned && t.folder.is_none()).map(|t| (t.order, Row::Tab { id: t.id, depth: 0 })).collect();
        top.extend(self.folders.iter().filter(|f| f.workspace == ws).map(|f| (f.order, Row::Folder { id: f.id })));
        top.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = Vec::new();
        for (_, r) in top {
            if let Row::Folder { id } = r {
                out.push(r.clone());
                let mut kids: Vec<&Tab> = self.tabs.iter().filter(|t| t.folder == Some(id)).collect();
                kids.sort_by(|a, b| a.order.total_cmp(&b.order));
                out.extend(kids.into_iter().map(|t| Row::Tab { id: t.id, depth: 1 }));
            } else {
                out.push(r);
            }
        }
        out
    }

    pub fn pinned(&self, ws: Id) -> Vec<Id> {
        let mut p: Vec<&Tab> = self.tabs.iter().filter(|t| t.workspace == ws && t.pinned).collect();
        p.sort_by(|a, b| a.order.total_cmp(&b.order));
        p.into_iter().map(|t| t.id).collect()
    }

    /// All tabs of a workspace in visual order (pinned first): used for Ctrl+Tab / Ctrl+1..9.
    pub fn display_order(&self, ws: Id) -> Vec<Id> {
        let mut v = self.pinned(ws);
        v.extend(self.rows(ws).into_iter().filter_map(|r| match r { Row::Tab { id, .. } => Some(id), _ => None }));
        v
    }

    // ---------- split view ----------
    pub fn split(&mut self, left: Id, right: Id) -> bool {
        let (Some(a), Some(b)) = (self.tab(left), self.tab(right)) else { return false };
        if left == right || a.workspace != b.workspace { return false; }
        let ws = a.workspace;
        if let Some(w) = self.workspace_mut(ws) {
            w.split = Some(Split { left, right, ratio: 0.5 });
            w.active_tab = Some(left);
        }
        true
    }
    pub fn unsplit(&mut self) { let ws = self.active_workspace; if let Some(w) = self.workspace_mut(ws) { w.split = None; } }

    // ---------- park for later ----------
    pub fn park(&mut self, tab: Id, remind_at: Option<u64>, note: &str) -> Option<(Id, Option<Id>)> {
        let t = self.tab(tab)?.clone();
        let id = self.alloc();
        self.parked.insert(0, Parked { id, url: t.url.clone(), title: t.title.clone(), favicon: t.favicon.clone(), parked_at: crate::now_ms(), remind_at, reminded: false, note: note.into() });
        let next = self.close_tab(tab);
        // Parking is deliberate; don't also list it under "recently closed".
        if self.closed.first().is_some_and(|c| c.url == t.url) { self.closed.remove(0); }
        Some((id, next))
    }
    pub fn unpark(&mut self, id: Id) -> Option<Id> {
        let pos = self.parked.iter().position(|p| p.id == id)?;
        let p = self.parked.remove(pos);
        let t = self.new_tab(&p.url, None, true);
        if let Some(tab) = self.tab_mut(t) { tab.title = p.title; tab.favicon = p.favicon; }
        Some(t)
    }
    /// Parked tabs whose reminder is due (marks them reminded).
    pub fn due_reminders(&mut self, now: u64) -> Vec<Parked> {
        let mut out = vec![];
        for p in &mut self.parked {
            if !p.reminded && p.remind_at.is_some_and(|r| r <= now) { p.reminded = true; out.push(p.clone()); }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s3() -> (Session, Id, Id, Id) {
        let mut s = Session::default();
        let a = s.new_tab("https://a.test/", None, true);
        let b = s.new_tab("https://b.test/", None, true);
        let c = s.new_tab("https://c.test/", None, true);
        (s, a, b, c)
    }

    #[test]
    fn new_tabs_append_and_activate() {
        let (s, a, b, c) = s3();
        assert_eq!(s.display_order(s.active_workspace), vec![a, b, c]);
        assert_eq!(s.active_tab(), Some(c));
    }

    #[test]
    fn child_tab_opens_below_opener() {
        let (mut s, a, b, c) = s3();
        let d = s.new_tab("https://d.test/", Some(a), false);
        assert_eq!(s.display_order(s.active_workspace), vec![a, d, b, c]);
        assert_eq!(s.active_tab(), Some(c));
    }

    #[test]
    fn closing_active_prefers_opener_then_neighbour() {
        let (mut s, a, b, c) = s3();
        let d = s.new_tab("https://d.test/", Some(a), true);
        assert_eq!(s.close_tab(d), Some(a));
        s.activate(b);
        assert_eq!(s.close_tab(b), Some(c));
        assert_eq!(s.close_tab(c), Some(a));
        assert_eq!(s.closed.len(), 3);
        let r = s.reopen_closed().unwrap();
        assert_eq!(s.tab(r).unwrap().url, "https://c.test/");
    }

    #[test]
    fn folders_and_drag_drop() {
        let (mut s, a, b, c) = s3();
        let f = s.add_folder("Research", false, "#888");
        assert!(s.move_item(a, &Target { folder: Some(f), ..Target::default() }));
        assert!(s.move_item(c, &Target { folder: Some(f), before: Some(a), ..Target::default() }));
        assert_eq!(s.rows(s.active_workspace), vec![Row::Tab { id: b, depth: 0 }, Row::Folder { id: f }, Row::Tab { id: c, depth: 1 }, Row::Tab { id: a, depth: 1 }]);
        // Move folder to top.
        assert!(s.move_item(f, &Target { before: Some(b), ..Target::default() }));
        assert_eq!(s.display_order(s.active_workspace), vec![c, a, b]);
        // Pin b.
        assert!(s.move_item(b, &Target { pinned: true, ..Target::default() }));
        assert_eq!(s.pinned(s.active_workspace), vec![b]);
        // Delete folder without closing: tabs move to top level in place.
        assert!(s.remove_folder(f, false).is_empty());
        assert_eq!(s.display_order(s.active_workspace), vec![b, c, a]);
    }

    #[test]
    fn repeated_insert_before_stays_ordered() {
        let mut s = Session::default();
        let first = s.new_tab("x", None, false);
        let mut ids = vec![first];
        for i in 0..80 {
            let t = s.new_tab(&format!("t{i}"), None, false);
            s.move_item(t, &Target { before: Some(ids[0]), ..Target::default() });
            ids.insert(0, t);
        }
        assert_eq!(s.display_order(s.active_workspace), ids);
    }

    #[test]
    fn workspaces_and_split() {
        let (mut s, a, b, _c) = s3();
        let w2 = s.add_workspace("Work", "💼", "#36c");
        assert!(s.move_item(b, &Target { workspace: Some(w2), ..Target::default() }));
        assert_eq!(s.tabs_in(w2).count(), 1);
        assert!(!s.split(a, b), "cannot split across workspaces");
        let d = s.new_tab("d", None, false);
        assert!(s.split(a, d));
        assert!(s.active().split.is_some());
        let closed = s.remove_workspace(w2);
        assert_eq!(closed, vec![b]);
    }

    #[test]
    fn park_and_remind() {
        let (mut s, a, _b, _c) = s3();
        let (pid, _) = s.park(a, Some(10), "read later").unwrap();
        assert!(s.tab(a).is_none());
        assert!(s.closed.iter().all(|c| c.url != "https://a.test/"));
        assert_eq!(s.due_reminders(5).len(), 0);
        assert_eq!(s.due_reminders(11).len(), 1);
        assert_eq!(s.due_reminders(12).len(), 0, "remind once");
        let t = s.unpark(pid).unwrap();
        assert_eq!(s.tab(t).unwrap().url, "https://a.test/");
    }

    #[test]
    fn normalize_repairs_dangling_refs() {
        let mut s: Session = serde_json::from_str(r#"{"workspaces":[{"id":5,"name":"x","active_tab":99}],"active_workspace":77,"tabs":[{"id":3,"workspace":42,"folder":8,"url":"u"}],"next_id":1}"#).unwrap();
        s.normalize();
        assert_eq!(s.active_workspace, 5);
        assert_eq!(s.tabs[0].workspace, 5);
        assert_eq!(s.tabs[0].folder, None);
        assert_eq!(s.workspaces[0].active_tab, None);
        assert!(s.next_id > 5);
    }
}
