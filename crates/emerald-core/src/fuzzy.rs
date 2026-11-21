//! Fuzzy matching for the command bar (Ctrl+K).
//!
//! Subsequence matching with bonuses for word starts, consecutive runs and prefix hits,
//! in the spirit of fzy/fzf but small. Case-insensitive. Returns `None` if `query` is not a
//! subsequence of `text`.

pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.chars().collect();
    let tl: Vec<char> = t.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    // Greedy forward match, then a backward pass to tighten the match window.
    let mut qi = 0;
    let mut end = 0;
    for (i, c) in tl.iter().enumerate() {
        if qi < q.len() && *c == q[qi] { qi += 1; if qi == q.len() { end = i; break; } }
    }
    if qi < q.len() { return None; }
    let mut start = end;
    let mut qj = q.len();
    for i in (0..=end).rev() {
        if qj > 0 && tl[i] == q[qj - 1] { qj -= 1; start = i; if qj == 0 { break; } }
    }
    // Score the tightened window.
    let mut s = 0i32;
    let mut qk = 0;
    let mut prev: Option<usize> = None;
    for i in start..=end {
        if qk < q.len() && tl[i] == q[qk] {
            s += 10;
            let boundary = i == 0 || matches!(t[i - 1], ' ' | '-' | '_' | '/' | '.' | ':' | '(' | '[') || (t[i].is_uppercase() && t[i - 1].is_lowercase());
            if boundary { s += 18; }
            if prev.is_some_and(|p| p + 1 == i) { s += 15; }
            prev = Some(i);
            qk += 1;
        }
    }
    s -= (end - start + 1 - q.len()) as i32; // gaps
    if start == 0 { s += 12; }
    s -= (t.len() as i32 / 12).min(10); // prefer shorter texts slightly
    Some(s)
}

/// Rank `items` by the best score over their fields; stable for ties.
pub fn rank<'a, T>(query: &str, items: &'a [T], fields: impl Fn(&T) -> Vec<&str>, limit: usize) -> Vec<(&'a T, i32)> {
    let mut out: Vec<(usize, &T, i32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| fields(it).iter().enumerate().filter_map(|(fi, f)| score(query, f).map(|s| s - fi as i32 * 4)).max().map(|s| (i, it, s)))
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    out.into_iter().take(limit).map(|(_, it, s)| (it, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basics() {
        assert!(score("gh", "GitHub").is_some());
        assert!(score("xyz", "GitHub").is_none());
        assert!(score("gh", "GitHub").unwrap() > score("gh", "Laughing").unwrap());
        assert!(score("set", "Settings").unwrap() > score("set", "Reset tabs").unwrap());
        assert_eq!(score("", "anything"), Some(0));
    }
    #[test]
    fn ranking() {
        let items = ["Open settings", "Toggle focus mode", "Focus: start pomodoro", "New tab"];
        let r = rank("focus", &items, |s| vec![*s], 10);
        assert_eq!(r.len(), 2);
        assert_eq!(*r[0].0, "Focus: start pomodoro");
    }
}
