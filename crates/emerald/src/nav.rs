//! Navigation hooks and periodic housekeeping on the shell.
use crate::shell::{self, Shell};
use crate::util::{self, later};
use cef::*;
use emerald_core::now_ms;
use emerald_core::session::Id;
use serde_json::json;

impl Shell {
    pub fn on_navigated(&mut self, id: Id, url: &str) {
        // Leaving a site clears any reader-mode state the UI shows for the tab.
        let _ = (id, url);
    }

    pub fn on_load_end(&mut self, id: Id, b: &Browser) {
        let url = util::browser_url(b);
        let title = self.session.tab(id).map(|t| t.title.clone()).unwrap_or_default();
        if let Some(h) = self.history.as_mut() { h.record(&url, &title, now_ms() as i64); }
        crate::extensions::on_page_loaded(self, id, b, &url);
        crate::passwords::on_page_loaded(self, id, b, &url);
        if self.reader_pending.remove(&id) { crate::reader::inject(b); }
    }

    /// A browser created by Chrome itself (e.g. `chrome.tabs.create` from an extension):
    /// close it and reopen the URL as an Emerald tab.
    pub fn adopt_foreign(&mut self, b: &Browser, url: &str) {
        let bid = b.identifier();
        if self.adopted.contains(&bid) { return; }
        self.adopted.insert(bid);
        if let Some(h) = b.host() { h.close_browser(1); }
        if self.closing { return; }
        let id = self.new_tab(Some(url), self.session.active_tab(), false);
        self.activate(id);
    }

    /// Runs every 15 s: timers, reminders, sleeping, distraction nudges.
    pub fn housekeeping(&mut self) {
        if let Some(msg) = self.focus.tick(&self.settings) {
            self.emit_ui("toast", json!({"text": msg, "kind": "timer", "sticky": true}));
            self.push_state();
        }
        for p in self.session.due_reminders(now_ms()) {
            self.emit_ui("toast", json!({"text": format!("Reminder: \u{201c}{}\u{201d} is waiting in Parked.", if p.title.is_empty() { &p.url } else { &p.title }), "kind": "reminder", "parked": p.id, "sticky": true}));
            self.mark_session_dirty();
        }
        self.sleep_sweep();
        self.distraction_check();
    }

    /// Gentle nudge after N minutes on a distracting site outside focus time.
    fn distraction_check(&mut self) {
        let lim = self.settings.adhd.distraction_nudge_min as u64;
        if lim == 0 || !self.settings.adhd.enabled { return; }
        let on_distracting = self.session.active_tab().and_then(|id| self.session.tab(id)).is_some_and(|t| crate::focus::FocusState::is_distracting(&self.settings, &t.url));
        if on_distracting { self.focus.distraction_ms += 15_000; } else { self.focus.distraction_ms = self.focus.distraction_ms.saturating_sub(5_000); }
        let now = now_ms();
        if self.focus.distraction_ms >= lim * 60_000 && now - self.focus.last_nudge > lim * 60_000 {
            self.focus.last_nudge = now;
            self.focus.distraction_ms = 0;
            self.emit_ui("nudge", json!({"kind": "distraction", "text": format!("You've been here about {lim} minutes. Still what you meant to do?")}));
        }
    }
}

pub fn start_housekeeping() {
    fn tick() {
        shell::with(|s| s.housekeeping());
        later(15_000, tick);
    }
    later(15_000, tick);
    // Timer display refresh while a Pomodoro is running (UI interpolates between pushes).
    fn timer() {
        shell::with(|s| if s.focus.pomo.is_some() { let st = s.focus.to_json(&s.settings); s.emit_all("focus", st); });
        later(1000, timer);
    }
    later(1000, timer);
}
