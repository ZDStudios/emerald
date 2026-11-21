//! The UI ⇄ Rust API. `handle` serves requests from emerald:// pages; `command` runs named
//! commands (keyboard shortcuts, command bar, menus).
use crate::client::Source;
use crate::native::Rect;
use crate::shell::{is_virtual_url, Shell};
use crate::util::{self, cs};
use anyhow::{anyhow, bail, Result};
use cef::*;
use emerald_core::session::{Id, Target};
use emerald_core::{now_ms, store};
use serde_json::{json, Value};

fn id_arg(a: &Value, k: &str) -> Result<Id> { a[k].as_u64().ok_or_else(|| anyhow!("missing {k}")) }
fn str_arg<'a>(a: &'a Value, k: &str) -> &'a str { a[k].as_str().unwrap_or("") }

/// Commands that only Emerald's privileged surfaces (shell UI, overlay, internal pages) may run.
pub fn handle(s: &mut Shell, src: Source, b: &Browser, cmd: &str, a: &Value) -> Result<Value> {
    let page = util::browser_url(b);
    Ok(match cmd {
        // ---------------------------------------------------------------- lifecycle / state
        "ready" => {
            if src == Source::Overlay { s.overlay_ready(); }
            s.state_json()
        }
        "state" => s.state_json(),
        "cmd" => { command(s, str_arg(a, "name"), &a["args"])?; Value::Null }
        "layout" => {
            let r = Rect { x: a["x"].as_f64().unwrap_or(0.0) as i32, y: a["y"].as_f64().unwrap_or(0.0) as i32, w: a["w"].as_f64().unwrap_or(0.0) as i32, h: a["h"].as_f64().unwrap_or(0.0) as i32 };
            s.set_content_rect(r, a["dpr"].as_f64().unwrap_or(1.0));
            Value::Null
        }
        // ---------------------------------------------------------------- navigation
        "navigate" => {
            let url = emerald_core::url::resolve_input(str_arg(a, "input"), &s.settings.search.url);
            let target = a["tab"].as_u64().or(s.session.active_tab());
            match (target, a["newTab"].as_bool().unwrap_or(false)) {
                (Some(id), false) => s.navigate(id, &url),
                _ => { let id = s.new_tab(Some(&url), None, false); s.activate(id); }
            }
            json!(url)
        }
        "tab.new" => {
            let url = a["url"].as_str().map(|u| emerald_core::url::resolve_input(u, &s.settings.search.url));
            json!(s.new_tab(url.as_deref(), None, a["background"].as_bool().unwrap_or(false)))
        }
        "tab.activate" => { s.activate(id_arg(a, "id")?); Value::Null }
        "tab.close" => { s.close_tab(id_arg(a, "id")?); Value::Null }
        "tab.sleep" => { s.sleep_tab(id_arg(a, "id")?); Value::Null }
        "tab.mute" => {
            let id = id_arg(a, "id")?;
            let muted = { let t = s.session.tab_mut(id).ok_or_else(|| anyhow!("no tab"))?; t.muted = !t.muted; t.muted };
            if let Some(h) = s.browser(id).and_then(|b| b.host()) { h.set_audio_muted(muted as i32); }
            s.after_tab_change();
            json!(muted)
        }
        "tab.move" => {
            let t: Target = serde_json::from_value(a["target"].clone())?;
            if !s.session.move_item(id_arg(a, "id")?, &t) { bail!("cannot move there"); }
            s.after_tab_change();
            Value::Null
        }
        "tab.pin" => {
            let id = id_arg(a, "id")?;
            let pinned = a["pinned"].as_bool().unwrap_or(true);
            s.session.move_item(id, &Target { pinned, ..Target::default() });
            s.after_tab_change();
            Value::Null
        }
        "tab.duplicate" => {
            let id = id_arg(a, "id")?;
            let url = s.session.tab(id).map(|t| t.url.clone()).unwrap_or_default();
            json!(s.new_tab(Some(&url), Some(id), false))
        }
        "tab.rename" => Value::Null,
        "tabs.closeOthers" => {
            let keep = id_arg(a, "id")?;
            let ids: Vec<Id> = s.session.tabs_in(s.session.active_workspace).filter(|t| t.id != keep && !t.pinned).map(|t| t.id).collect();
            for id in ids { s.close_tab(id); }
            Value::Null
        }
        // ---------------------------------------------------------------- folders / spaces / split
        "folder.new" => {
            let id = s.session.add_folder(a["name"].as_str().unwrap_or("New folder"), a["group"].as_bool().unwrap_or(false), a["color"].as_str().unwrap_or("#5b8def"));
            if let Some(tabs) = a["tabs"].as_array() {
                for t in tabs.iter().filter_map(Value::as_u64) { s.session.move_item(t, &Target { folder: Some(id), ..Target::default() }); }
            }
            s.after_tab_change();
            json!(id)
        }
        "folder.update" => {
            let id = id_arg(a, "id")?;
            let f = s.session.folder_mut(id).ok_or_else(|| anyhow!("no folder"))?;
            if let Some(n) = a["name"].as_str() { f.name = n.chars().take(80).collect(); }
            if let Some(c) = a["color"].as_str() { f.color = c.into(); }
            if let Some(c) = a["collapsed"].as_bool() { f.collapsed = c; }
            s.after_tab_change();
            Value::Null
        }
        "folder.delete" => {
            let closed = s.session.remove_folder(id_arg(a, "id")?, a["closeTabs"].as_bool().unwrap_or(false));
            for id in closed { s.close_tab(id); }
            s.after_tab_change();
            Value::Null
        }
        "ws.new" => {
            let id = s.session.add_workspace(a["name"].as_str().unwrap_or("Space"), a["icon"].as_str().unwrap_or("✨"), a["color"].as_str().unwrap_or("#5b8def"));
            s.session.active_workspace = id;
            s.after_tab_change();
            json!(id)
        }
        "ws.update" => {
            let id = id_arg(a, "id")?;
            let w = s.session.workspace_mut(id).ok_or_else(|| anyhow!("no space"))?;
            if let Some(n) = a["name"].as_str() { w.name = n.chars().take(60).collect(); }
            if let Some(n) = a["icon"].as_str() { w.icon = n.chars().take(4).collect(); }
            if let Some(n) = a["color"].as_str() { w.color = n.into(); }
            s.after_tab_change();
            Value::Null
        }
        "ws.delete" => {
            let closed = s.session.remove_workspace(id_arg(a, "id")?);
            for id in closed { s.close_tab(id); }
            s.after_tab_change();
            Value::Null
        }
        "ws.switch" => {
            let id = id_arg(a, "id")?;
            if s.session.workspace(id).is_some() { s.session.active_workspace = id; }
            s.after_tab_change();
            if let Some(t) = s.session.active_tab() { s.focus_tab(t); }
            Value::Null
        }
        "split" => {
            let (l, r) = (id_arg(a, "left")?, id_arg(a, "right")?);
            if !s.session.split(l, r) { bail!("cannot split those tabs"); }
            s.after_tab_change();
            Value::Null
        }
        "split.ratio" => {
            let ws = s.session.active_workspace;
            if let Some(sp) = s.session.workspace_mut(ws).and_then(|w| w.split.as_mut()) { sp.ratio = a["ratio"].as_f64().unwrap_or(0.5).clamp(0.2, 0.8); }
            s.apply_layout();
            s.mark_session_dirty();
            Value::Null
        }
        "unsplit" => { s.session.unsplit(); s.after_tab_change(); Value::Null }
        // ---------------------------------------------------------------- parking & notes
        "park" => {
            let id = id_arg(a, "id")?;
            let remind = a["remindInMin"].as_u64().map(|m| now_ms() + m * 60_000).or(a["remindAt"].as_u64());
            let (_, next) = s.session.park(id, remind, str_arg(a, "note")).ok_or_else(|| anyhow!("no tab"))?;
            if let Some(b) = s.browser(id) { if let Some(h) = b.host() { h.close_browser(1); } }
            s.live.remove(&id);
            if let Some(n) = next { s.session.activate(n); }
            s.after_tab_change();
            Value::Null
        }
        "unpark" => { let id = s.session.unpark(id_arg(a, "id")?).ok_or_else(|| anyhow!("not parked"))?; s.after_tab_change(); json!(id) }
        "parked.delete" => { let id = id_arg(a, "id")?; s.session.parked.retain(|p| p.id != id); s.after_tab_change(); Value::Null }
        "notes.list" => json!(load_notes(s)),
        "notes.add" => { add_note(s, str_arg(a, "text"), str_arg(a, "url")); json!(load_notes(s)) }
        "notes.update" => {
            let mut n = load_notes(s);
            let id = id_arg(a, "id")?;
            if let Some(x) = n.iter_mut().find(|x| x["id"] == id) {
                if let Some(t) = a["text"].as_str() { x["text"] = json!(t); }
                if let Some(d) = a["done"].as_bool() { x["done"] = json!(d); }
            }
            save_notes(s, &n);
            json!(n)
        }
        "notes.delete" => { let id = id_arg(a, "id")?; let mut n = load_notes(s); n.retain(|x| x["id"] != id); save_notes(s, &n); json!(n) }
        // ---------------------------------------------------------------- focus tools
        "focus.mode" => { s.focus_mode = a["on"].as_bool().unwrap_or(!s.focus_mode); s.after_tab_change(); json!(s.focus_mode) }
        "pomodoro.start" => { s.focus.start_pomodoro(&s.settings); s.push_state(); Value::Null }
        "pomodoro.pause" => { s.focus.toggle_pause(); s.push_state(); Value::Null }
        "pomodoro.stop" => { s.focus.pomo = None; s.push_state(); Value::Null }
        "pomodoro.skip" => { if let Some(p) = s.focus.pomo.as_mut() { p.ends = 0; } let m = s.focus.tick(&s.settings); s.push_state(); json!(m) }
        "focus.session" => {
            let min = a["minutes"].as_u64().unwrap_or(0);
            s.focus.session_until = (min > 0).then(|| now_ms() + min * 60_000);
            s.push_state();
            Value::Null
        }
        "focus.allowOnce" => {
            let url = str_arg(a, "url");
            if let Some(h) = emerald_core::url::host(url) { s.focus.allow.push((h, now_ms() + a["minutes"].as_u64().unwrap_or(5) * 60_000)); }
            if let Some(id) = s.session.active_tab() { s.navigate(id, url); }
            Value::Null
        }
        // ---------------------------------------------------------------- settings & themes
        "settings.get" => json!(s.settings),
        "settings.set" => {
            let mut cur = serde_json::to_value(&s.settings)?;
            merge(&mut cur, &a["patch"]);
            s.settings = serde_json::from_value(cur)?;
            if let Some((_, url)) = emerald_core::settings::SEARCH_ENGINES.iter().find(|(n, _)| *n == s.settings.search.engine) {
                s.settings.search.url = (*url).to_string();
            }
            s.save_settings();
            crate::privacy::configure(s.settings.privacy.block_ads_trackers, s.settings.privacy.send_do_not_track);
            crate::privacy::apply_prefs(&s.settings);
            s.apply_theme();
            s.push_state();
            json!(s.settings)
        }
        "themes.list" => json!(s.themes),
        "theme.css" => {
            let t: emerald_core::theme::Theme = serde_json::from_value(a["theme"].clone())?;
            let t = t.validated(false).map_err(|e| anyhow!(e))?;
            json!(t.to_css(s.themes.iter().find(|x| x.id == "emerald-dark")))
        }
        "theme.save" => {
            let t: emerald_core::theme::Theme = serde_json::from_value(a["theme"].clone())?;
            let mut t = t.validated(false).map_err(|e| anyhow!(e))?;
            if s.themes.iter().any(|x| x.id == t.id && x.builtin) { t.id = format!("{}-custom", t.id); }
            t.builtin = false;
            store::save_json(&s.paths.themes().join(format!("{}.json", t.id)), &t)?;
            s.reload_themes();
            if a["apply"].as_bool().unwrap_or(true) {
                s.settings.theme = t.id.clone();
                // A theme may carry a preferred tab layout.
                match t.tab_layout.as_deref() {
                    Some("vertical") => s.settings.layout.tabs = emerald_core::settings::TabLayout::Vertical,
                    Some("horizontal") => s.settings.layout.tabs = emerald_core::settings::TabLayout::Horizontal,
                    _ => {}
                }
                s.save_settings();
            }
            s.apply_theme();
            s.push_state();
            json!(t.id)
        }
        "theme.delete" => {
            let id = str_arg(a, "id");
            if s.themes.iter().any(|x| x.id == id && x.builtin) { bail!("built-in themes can't be deleted"); }
            if id.is_empty() || id.contains(['/', '\\', '.']) { bail!("bad id"); }
            let _ = std::fs::remove_file(s.paths.themes().join(format!("{id}.json")));
            s.reload_themes();
            if s.settings.theme == id { s.settings.theme = "emerald-dark".into(); s.save_settings(); }
            s.apply_theme();
            s.push_state();
            Value::Null
        }
        "theme.import" => {
            let themes = emerald_core::theme::parse_import(str_arg(a, "json")).map_err(|e| anyhow!(e))?;
            let mut ids = vec![];
            for mut t in themes {
                if s.themes.iter().any(|x| x.id == t.id && x.builtin) { t.id = format!("{}-imported", t.id); }
                store::save_json(&s.paths.themes().join(format!("{}.json", t.id)), &t)?;
                ids.push(t.id);
            }
            s.reload_themes();
            s.push_state();
            json!(ids)
        }
        "theme.export" => {
            let ids: Vec<String> = a["ids"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let themes: Vec<_> = s.themes.iter().filter(|t| ids.contains(&t.id)).cloned().collect();
            json!(emerald_core::theme::export_pack(a["name"].as_str().unwrap_or("Emerald themes"), &themes))
        }
        // ---------------------------------------------------------------- window
        "win.minimize" => { if let Some(w) = &s.window { w.minimize(); } Value::Null }
        "win.maximize" => { if let Some(w) = &s.window { if w.is_maximized() != 0 { w.restore() } else { w.maximize() } } Value::Null }
        "win.close" => { if let Some(w) = &s.window { w.close(); } Value::Null }
        "overlay.open" => { s.show_overlay(str_arg(a, "mode"), a["arg"].clone()); Value::Null }
        "overlay.close" => { s.hide_overlay(); Value::Null }
        "ui.focus" => { if let Some(h) = s.ui.as_ref().and_then(|b| b.host()) { h.set_focus(1); } Value::Null }
        // ---------------------------------------------------------------- search & data
        "search" => search(s, str_arg(a, "q")),
        "history.list" => json!(s.history.as_ref().map(|h| h.recent(str_arg(a, "q"), a["before"].as_i64().unwrap_or(i64::MAX), 200)).unwrap_or_default()),
        "history.delete" => { if let Some(h) = s.history.as_mut() { h.delete_url(str_arg(a, "url")); } Value::Null }
        "history.clear" => { if let Some(h) = s.history.as_mut() { h.clear_since(a["since"].as_i64().unwrap_or(0)); } Value::Null }
        "bookmarks.list" => json!(crate::bookmarks::load(&s.paths)),
        "bookmarks.add" => { crate::bookmarks::add(&s.paths, str_arg(a, "url"), str_arg(a, "title"), str_arg(a, "folder")); s.push_state(); json!(crate::bookmarks::load(&s.paths)) }
        "bookmarks.delete" => { crate::bookmarks::delete(&s.paths, str_arg(a, "url")); json!(crate::bookmarks::load(&s.paths)) }
        "downloads.list" => json!(s.downloads),
        "downloads.control" => { crate::downloads::control(a["id"].as_u64().unwrap_or(0) as u32, str_arg(a, "action")); Value::Null }
        "downloads.open" => { open_path(str_arg(a, "path"), a["reveal"].as_bool().unwrap_or(false)); Value::Null }
        "downloads.clear" => { s.downloads.retain(|d| d.state == "in_progress"); s.push_state(); Value::Null }
        "memory" => crate::memory::report(s),
        "memory.free" => json!(s.sleep_all_background()),
        "permission.respond" => {
            crate::privacy::respond(id_arg(a, "id")?, a["allow"].as_bool().unwrap_or(false), a["remember"].as_bool().unwrap_or(true), &s.paths.file("permissions.json"));
            Value::Null
        }
        "permissions.list" => crate::privacy::decisions_json(),
        "permissions.set" => {
            crate::privacy::set_decision(&s.paths.file("permissions.json"), str_arg(a, "origin"), a["perms"].as_u64().unwrap_or(0) as u32, a["allow"].as_bool());
            crate::privacy::decisions_json()
        }
        "privacy.stats" => json!({"blocked": crate::privacy::BLOCKED.load(std::sync::atomic::Ordering::Relaxed)}),
        "find" => {
            let text = str_arg(a, "text").to_string();
            let id = s.session.active_tab().ok_or_else(|| anyhow!("no tab"))?;
            if let Some(h) = s.browser(id).and_then(|b| b.host()) {
                if text.is_empty() { h.stop_finding(1); } else { h.find(Some(&cs(&text)), a["forward"].as_bool().unwrap_or(true) as i32, a["matchCase"].as_bool().unwrap_or(false) as i32, a["next"].as_bool().unwrap_or(false) as i32); }
            }
            s.find = (!text.is_empty()).then_some((id, text));
            Value::Null
        }
        "find.stop" => {
            if let Some((id, _)) = s.find.take() { if let Some(h) = s.browser(id).and_then(|b| b.host()) { h.stop_finding(1); } }
            if let Some(id) = s.session.active_tab() { s.focus_tab(id); }
            Value::Null
        }
        "import.detect" => crate::import::detect(),
        "import.run" => crate::import::run(s, a)?,
        "import.file" => crate::import::run_file(s, a)?,
        "ext.list" => crate::extensions::list_json(s),
        "ext.action" => crate::extensions::action(s, a)?,
        "ext.popup" => { crate::extensions::open_popup(s, str_arg(a, "id"), a["x"].as_f64().unwrap_or(0.0), a["y"].as_f64().unwrap_or(0.0)); Value::Null }
        "wizard.done" => { s.settings.first_run_done = true; s.save_settings(); s.push_state(); Value::Null }
        "account.signin" => {
            let url = match str_arg(a, "provider") { "google" => "https://accounts.google.com/ServiceLogin?continue=https://myaccount.google.com/", _ => "https://login.live.com/" };
            let id = s.new_tab(Some(url), None, false);
            s.activate(id);
            Value::Null
        }
        "open" => {
            let url = str_arg(a, "url");
            let id = s.new_tab(Some(url), None, a["background"].as_bool().unwrap_or(false));
            if !a["background"].as_bool().unwrap_or(false) { s.activate(id); }
            json!(id)
        }
        "page.info" => json!({"url": page}),
        c if c.starts_with("pw.") => crate::passwords::api(s, c, a)?,
        _ => bail!("unknown api call {cmd}"),
    })
}

/// JSON merge-patch (RFC 7386).
pub fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() { t.remove(k); } else { merge(t.entry(k.clone()).or_insert(Value::Null), v); }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

fn open_path(path: &str, reveal: bool) {
    let p = std::path::Path::new(path);
    let target = if reveal { p.parent().unwrap_or(p) } else { p };
    #[cfg(target_os = "windows")]
    let r = if reveal { std::process::Command::new("explorer").arg(format!("/select,{}", path)).spawn() } else { std::process::Command::new("explorer").arg(target).spawn() };
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").args(if reveal { vec!["-R", path] } else { vec![path] }).spawn();
    #[cfg(target_os = "linux")]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    if let Err(e) = r { eprintln!("emerald: open {path}: {e}"); }
    let _ = target;
}

pub fn load_notes(s: &Shell) -> Vec<Value> { store::load_json(&s.paths.file("notes.json")) }
fn save_notes(s: &Shell, n: &[Value]) { let _ = store::save_json(&s.paths.file("notes.json"), &n); }

pub fn add_note(s: &mut Shell, text: &str, url: &str) {
    let text = text.trim();
    if text.is_empty() { return; }
    let mut n = load_notes(s);
    let id = n.iter().filter_map(|x| x["id"].as_u64()).max().unwrap_or(0) + 1;
    n.insert(0, json!({"id": id, "text": text, "url": url, "created": now_ms(), "done": false}));
    save_notes(s, &n);
    s.emit_all("notes", json!(n));
    s.emit_ui("toast", json!({"text": "Saved to quick notes", "kind": "info"}));
}

/// Unified search for the command bar: open tabs, parked tabs, bookmarks, history.
fn search(s: &Shell, q: &str) -> Value {
    use emerald_core::fuzzy::rank;
    let tabs: Vec<&emerald_core::session::Tab> = s.session.tabs.iter().filter(|t| !is_virtual_url(&t.url)).collect();
    let tab_hits: Vec<Value> = rank(q, &tabs, |t| vec![t.title.as_str(), t.url.as_str()], 8).into_iter().map(|(t, sc)| json!({"kind": "tab", "id": t.id, "title": t.title, "url": t.url, "favicon": t.favicon, "score": sc + 40, "workspace": t.workspace})).collect();
    let parked: Vec<Value> = rank(q, &s.session.parked, |p| vec![p.title.as_str(), p.url.as_str()], 4).into_iter().map(|(p, sc)| json!({"kind": "parked", "id": p.id, "title": p.title, "url": p.url, "favicon": p.favicon, "score": sc + 20})).collect();
    let bm = crate::bookmarks::load(&s.paths);
    let bms: Vec<Value> = rank(q, &bm, |b| vec![b.title.as_str(), b.url.as_str()], 6).into_iter().map(|(b, sc)| json!({"kind": "bookmark", "title": b.title, "url": b.url, "score": sc + 25})).collect();
    let hist: Vec<Value> = if q.trim().is_empty() { vec![] } else {
        s.history.as_ref().map(|h| h.search(q, 8)).unwrap_or_default().into_iter().map(|e| json!({"kind": "history", "title": e.title, "url": e.url, "score": (e.visits.min(20) as i32) + 10})).collect()
    };
    json!({"tabs": tab_hits, "parked": parked, "bookmarks": bms, "history": hist})
}

/// Named commands (keyboard shortcuts, command bar).
pub fn command(s: &mut Shell, name: &str, args: &Value) -> Result<()> {
    let active = s.session.active_tab();
    let ab = s.active_browser();
    let host = ab.as_ref().and_then(|b| b.host());
    match name {
        "noop" => {}
        "tab.new" => {
            let id = s.new_tab(None, None, false);
            s.activate(id);
            // The start page is drawn by the shell: focus its search box.
            if let Some(h) = s.ui.as_ref().and_then(|b| b.host()) { h.set_focus(1); }
            s.emit_ui("focusStart", json!({}));
        }
        "tab.close" => if let Some(id) = active { s.close_tab(id) },
        "tab.reopen" => { if s.session.reopen_closed().is_some() { s.after_tab_change(); } }
        "tab.next" | "tab.prev" => {
            let order = s.session.display_order(s.session.active_workspace);
            if let (Some(a), false) = (active, order.is_empty()) {
                let i = order.iter().position(|x| *x == a).unwrap_or(0) as i64;
                let n = order.len() as i64;
                let j = if name == "tab.next" { (i + 1) % n } else { (i - 1 + n) % n };
                s.activate(order[j as usize]);
            }
        }
        "tab.last" => { let o = s.session.display_order(s.session.active_workspace); if let Some(l) = o.last() { s.activate(*l); } }
        n if n.starts_with("tab.n") => {
            let k: usize = n[5..].parse().unwrap_or(1);
            let o = s.session.display_order(s.session.active_workspace);
            if let Some(id) = o.get(k - 1) { s.activate(*id); }
        }
        "tab.park" => if let Some(id) = active { s.show_overlay("park", json!({"tab": id})) },
        "tab.sleep_others" | "memory.free" => { let n = s.sleep_all_background(); s.emit_ui("toast", json!({"text": format!("Put {n} background tab{} to sleep", if n == 1 { "" } else { "s" }), "kind": "info"})); }
        "tab.pin" => if let Some(id) = active {
            let pinned = s.session.tab(id).is_some_and(|t| t.pinned);
            s.session.move_item(id, &Target { pinned: !pinned, ..Target::default() });
            s.after_tab_change();
        },
        "tab.mute" => if let (Some(id), Some(h)) = (active, &host) {
            if let Some(t) = s.session.tab_mut(id) { t.muted = !t.muted; h.set_audio_muted(t.muted as i32); }
            s.after_tab_change();
        },
        "nav.back" => if let Some(b) = &ab { b.go_back() },
        "nav.forward" => if let Some(b) = &ab { b.go_forward() },
        "nav.reload" => match (&ab, active) {
            (Some(b), _) => b.reload(),
            (None, Some(id)) => { s.live.remove(&id); s.apply_layout(); }
            _ => {}
        },
        "nav.reload_hard" => if let Some(b) = &ab { b.reload_ignore_cache() },
        "nav.stop" => if let Some(b) = &ab { b.stop_load() },
        "focus.address" => {
            let url = active.and_then(|id| s.session.tab(id)).map(|t| t.url.clone()).unwrap_or_default();
            s.show_overlay("palette", json!({"url": url}));
        }
        "palette" => s.show_overlay("palette", args.clone()),
        "find" => {
            if let Some(h) = s.ui.as_ref().and_then(|b| b.host()) { h.set_focus(1); }
            s.emit_ui("find", json!({}));
        }
        "devtools" => if let Some(h) = &host {
            if h.has_dev_tools() != 0 { h.close_dev_tools(); } else { h.show_dev_tools(Some(&WindowInfo::default()), None, None, None); }
        },
        "print" => if let Some(h) = &host { h.print() },
        "zoom.in" | "zoom.out" | "zoom.reset" => if let (Some(h), Some(id)) = (&host, active) {
            let z = match name { "zoom.in" => (h.zoom_level() + 0.5).min(5.0), "zoom.out" => (h.zoom_level() - 0.5).max(-5.0), _ => 0.0 };
            h.set_zoom_level(z);
            if let Some(t) = s.session.tab_mut(id) { t.zoom = z; }
            s.emit_ui("toast", json!({"text": format!("Zoom {}%", (1.2f64.powf(z) * 100.0).round()), "kind": "info", "ms": 1200}));
            s.mark_session_dirty();
        },
        "focus.toggle" => { s.focus_mode = !s.focus_mode; s.after_tab_change(); }
        "sidebar.toggle" => s.emit_ui("toggleSidebar", json!({})),
        "note.quick" => s.show_overlay("note", json!({})),
        "memory" => s.show_overlay("memory", json!({})),
        "split.toggle" => {
            let ws = s.session.active_workspace;
            if s.session.active().split.is_some() { s.session.unsplit(); }
            else if let Some(a) = active {
                // Split with the most recently used other tab in this space.
                let other = s.session.tabs_in(ws).filter(|t| t.id != a).max_by_key(|t| t.last_active).map(|t| t.id);
                match other { Some(o) => { s.session.split(a, o); } None => { let n = s.session.new_tab("emerald://start/", Some(a), false); s.session.split(a, n); } }
            }
            s.after_tab_change();
        }
        "reader.toggle" => if let Some(b) = &ab { crate::reader::inject(b, s.settings.adhd.dyslexia_font) },
        "video.focus" => if let Some(b) = &ab { util::exec_js(b, crate::reader::VIDEO_FOCUS_JS) },
        "history" => open_internal(s, "emerald://history/"),
        "downloads" => open_internal(s, "emerald://downloads/"),
        "settings" => open_internal(s, "emerald://settings/"),
        "extensions" => open_internal(s, "emerald://extensions/"),
        "themes" => open_internal(s, "emerald://themes/"),
        "bookmarks" => open_internal(s, "emerald://bookmarks/"),
        "welcome" => open_internal(s, "emerald://welcome/"),
        "bookmark.add" => if let Some(t) = active.and_then(|id| s.session.tab(id)) {
            if !is_virtual_url(&t.url) {
                crate::bookmarks::add(&s.paths, &t.url.clone(), &t.title.clone(), "");
                s.emit_ui("toast", json!({"text": "Bookmarked", "kind": "info"}));
            }
        },
        "fullscreen" => if let Some(w) = &s.window { w.set_fullscreen((w.is_fullscreen() == 0) as i32) },
        "pomodoro.start" => { s.focus.start_pomodoro(&s.settings); s.push_state(); }
        "pomodoro.stop" => { s.focus.pomo = None; s.push_state(); }
        other => bail!("unknown command {other}"),
    }
    Ok(())
}

/// Open an internal page, reusing an existing tab showing it.
pub fn open_internal(s: &mut Shell, url: &str) {
    let existing = s.session.tabs_in(s.session.active_workspace).find(|t| t.url.starts_with(url)).map(|t| t.id);
    match existing {
        Some(id) => s.activate(id),
        None => {
            // Replace an empty start tab rather than adding another.
            match s.session.active_tab().filter(|id| s.session.tab(*id).is_some_and(|t| is_virtual_url(&t.url))) {
                Some(id) => s.navigate(id, url),
                None => { let id = s.new_tab(Some(url), None, false); s.activate(id); }
            }
        }
    }
}
