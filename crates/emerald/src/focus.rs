//! ADHD focus tools: Pomodoro timer, focus sessions, scheduled focus time and the site
//! blocker that runs during them.
use emerald_core::now_ms;
use emerald_core::settings::Settings;
use emerald_core::url::{encode_component, host, host_matches};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Work, ShortBreak, LongBreak }

#[derive(Debug, Clone)]
pub struct Pomo {
    pub phase: Phase,
    pub started: u64,
    pub ends: u64,
    pub cycle: u32,
    pub paused_left: Option<u64>,
}

#[derive(Debug, Default)]
pub struct FocusState {
    pub pomo: Option<Pomo>,
    /// Manual focus session ("block distractions for 45 minutes").
    pub session_until: Option<u64>,
    /// Allow-once exceptions: host → until.
    pub allow: Vec<(String, u64)>,
    /// Time spent on distracting sites outside focus time (host, ms) for gentle nudges.
    pub distraction_ms: u64,
    pub last_nudge: u64,
}

impl FocusState {
    pub fn schedule_active(&self, s: &Settings) -> Option<String> {
        let (dow, minute) = local_now();
        s.adhd.schedules.iter().find(|sc| {
            sc.enabled && sc.days[dow as usize] && if sc.start_min <= sc.end_min { minute >= sc.start_min && minute < sc.end_min } else { minute >= sc.start_min || minute < sc.end_min }
        }).map(|sc| sc.name.clone())
    }

    /// Why sites are currently blocked, if they are.
    pub fn blocking_reason(&self, s: &Settings) -> Option<String> {
        let now = now_ms();
        if self.session_until.is_some_and(|u| u > now) { return Some("Focus session".into()); }
        if let Some(p) = &self.pomo {
            if p.phase == Phase::Work && p.paused_left.is_none() && s.adhd.pomodoro.block_during_work { return Some("Pomodoro".into()); }
        }
        self.schedule_active(s)
    }

    pub fn is_distracting(s: &Settings, url: &str) -> bool {
        let Some(h) = host(url) else { return false };
        s.adhd.blocked_sites.iter().any(|p| host_matches(&h, p))
    }

    /// If `url` should be blocked right now, the URL of the calm "blocked" page.
    pub fn block_redirect(&self, s: &Settings, url: &str) -> Option<String> {
        if !s.adhd.enabled || !(url.starts_with("http://") || url.starts_with("https://")) { return None; }
        let reason = self.blocking_reason(s)?;
        let h = host(url)?;
        if !s.adhd.blocked_sites.iter().any(|p| host_matches(&h, p)) { return None; }
        let now = now_ms();
        if self.allow.iter().any(|(a, until)| *until > now && host_matches(&h, a)) { return None; }
        let until = self.session_until.filter(|u| *u > now).or(self.pomo.as_ref().map(|p| p.ends)).unwrap_or(0);
        Some(format!("emerald://blocked/?url={}&reason={}&until={until}", encode_component(url), encode_component(&reason)))
    }

    pub fn start_pomodoro(&mut self, s: &Settings) {
        let now = now_ms();
        self.pomo = Some(Pomo { phase: Phase::Work, started: now, ends: now + s.adhd.pomodoro.work_min as u64 * 60_000, cycle: 1, paused_left: None });
    }

    pub fn toggle_pause(&mut self) {
        let now = now_ms();
        if let Some(p) = &mut self.pomo {
            match p.paused_left.take() {
                Some(left) => { p.ends = now + left; }
                None => { p.paused_left = Some(p.ends.saturating_sub(now)); }
            }
        }
    }

    /// Advance the timer. Returns a message when a phase ends.
    pub fn tick(&mut self, s: &Settings) -> Option<String> {
        let now = now_ms();
        let pc = &s.adhd.pomodoro;
        let p = self.pomo.as_mut()?;
        if p.paused_left.is_some() || now < p.ends { return None; }
        let (phase, minutes, msg) = match p.phase {
            Phase::Work if p.cycle % pc.cycles_before_long.max(1) == 0 => (Phase::LongBreak, pc.long_break_min, "Nice work. Take a longer break: stretch, water, look away from the screen."),
            Phase::Work => (Phase::ShortBreak, pc.short_break_min, "Focus block done. Short break time."),
            _ => { p.cycle += 1; (Phase::Work, pc.work_min, "Break's over. Ready for the next focus block?") }
        };
        p.phase = phase;
        p.started = now;
        p.ends = now + minutes as u64 * 60_000;
        Some(msg.into())
    }

    pub fn to_json(&self, s: &Settings) -> Value {
        let now = now_ms();
        json!({
            "pomodoro": self.pomo.as_ref().map(|p| json!({
                "phase": match p.phase { Phase::Work => "work", Phase::ShortBreak => "short", Phase::LongBreak => "long" },
                "ends": p.ends, "started": p.started, "cycle": p.cycle, "paused": p.paused_left.is_some(),
                "left": p.paused_left.unwrap_or(p.ends.saturating_sub(now)),
            })),
            "sessionUntil": self.session_until.filter(|u| *u > now),
            "blocking": self.blocking_reason(s),
            "schedule": self.schedule_active(s),
        })
    }
}

/// Borrow-free snapshot of the blocker for callbacks that may fire while the shell is busy.
static SNAPSHOT: std::sync::RwLock<Option<(Settings, Option<u64>, Option<(Phase, bool, u64)>, Vec<(String, u64)>)>> = std::sync::RwLock::new(None);

pub fn publish(f: &FocusState, s: &Settings) {
    let pomo = f.pomo.as_ref().map(|p| (p.phase, p.paused_left.is_some(), p.ends));
    if let Ok(mut g) = SNAPSHOT.write() { *g = Some((s.clone(), f.session_until, pomo, f.allow.clone())); }
}

/// Same as `FocusState::block_redirect`, from the snapshot.
pub fn block_redirect_snapshot(url: &str) -> Option<String> {
    let g = SNAPSHOT.read().ok()?;
    let (s, session_until, pomo, allow) = g.as_ref()?;
    let tmp = FocusState {
        pomo: pomo.map(|(phase, paused, ends)| Pomo { phase, started: 0, ends, cycle: 1, paused_left: paused.then_some(1) }),
        session_until: *session_until,
        allow: allow.clone(),
        ..Default::default()
    };
    tmp.block_redirect(s, url)
}

/// (weekday Monday=0, minutes after local midnight).
pub fn local_now() -> (u32, u32) {
    use chrono::{Datelike, Local, Timelike};
    let n = Local::now();
    (n.weekday().num_days_from_monday(), n.hour() * 60 + n.minute())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_only_listed_sites_during_focus() {
        let mut st = Settings::default();
        st.adhd.schedules.clear();
        let mut f = FocusState::default();
        assert!(f.block_redirect(&st, "https://www.youtube.com/watch?v=x").is_none(), "no focus → no block");
        f.session_until = Some(now_ms() + 60_000);
        let r = f.block_redirect(&st, "https://m.youtube.com/watch?v=x").unwrap();
        assert!(r.starts_with("emerald://blocked/?url=https%3A%2F%2Fm.youtube.com"));
        assert!(f.block_redirect(&st, "https://docs.rs/").is_none());
        f.allow.push(("youtube.com".into(), now_ms() + 1000));
        assert!(f.block_redirect(&st, "https://youtube.com/").is_none(), "allow-once works");
    }
    #[test]
    fn pomodoro_cycles() {
        let mut st = Settings::default();
        st.adhd.pomodoro.cycles_before_long = 2;
        let mut f = FocusState::default();
        f.start_pomodoro(&st);
        f.pomo.as_mut().unwrap().ends = 0;
        assert!(f.tick(&st).is_some());
        assert_eq!(f.pomo.as_ref().unwrap().phase, Phase::ShortBreak);
        f.pomo.as_mut().unwrap().ends = 0;
        f.tick(&st);
        assert_eq!(f.pomo.as_ref().unwrap().phase, Phase::Work);
        assert_eq!(f.pomo.as_ref().unwrap().cycle, 2);
        f.pomo.as_mut().unwrap().ends = 0;
        f.tick(&st);
        assert_eq!(f.pomo.as_ref().unwrap().phase, Phase::LongBreak);
    }
}
