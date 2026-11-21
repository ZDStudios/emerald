//! Turning what the user typed into a URL, and small URL helpers.

/// Resolve address-bar input: URL, bare domain, localhost, IP or search.
pub fn resolve_input(input: &str, search_template: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return "emerald://start/".into();
    }
    let lower = s.to_ascii_lowercase();
    let has_scheme = ["http://", "https://", "file://", "emerald://", "chrome://", "chrome-extension://", "about:", "data:", "view-source:", "devtools://"]
        .iter()
        .any(|p| lower.starts_with(p));
    if has_scheme {
        return s.into();
    }
    if !s.contains(char::is_whitespace) {
        let host = s.split(['/', '?', '#']).next().unwrap_or("");
        let host_no_port = host.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty()).map(|(h, _)| h).unwrap_or(host);
        let looks_like_host = host_no_port.eq_ignore_ascii_case("localhost")
            || host_no_port.parse::<std::net::Ipv4Addr>().is_ok()
            || (host_no_port.starts_with('[') && host_no_port.ends_with(']'))
            || (host_no_port.contains('.')
                && !host_no_port.starts_with('.')
                && !host_no_port.ends_with('.')
                && host_no_port.rsplit('.').next().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic() || c == '-') || tld.starts_with("xn--"))
                && host_no_port.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-'));
        if looks_like_host {
            let local = host_no_port.eq_ignore_ascii_case("localhost") || host_no_port.starts_with("127.") || host_no_port.starts_with("192.168.") || host_no_port.starts_with("10.");
            return format!("{}://{s}", if local { "http" } else { "https" });
        }
    }
    search_template.replace("%s", &encode_component(s))
}

/// Percent-encode for a query component (RFC 3986 unreserved characters kept).
pub fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn decode_component(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => { out.push(b' '); i += 1; }
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => { out.push(v); i += 3; }
                    Err(_) => { out.push(b'%'); i += 1; }
                }
            }
            c => { out.push(c); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse `a=b&c=d` (no leading `?`).
pub fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&').filter(|p| !p.is_empty()).map(|p| {
        let (k, v) = p.split_once('=').unwrap_or((p, ""));
        (decode_component(k), decode_component(v))
    }).collect()
}

/// Host of an http(s) URL, lower-cased, without `www.` and port.
pub fn host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let auth = rest.split(['/', '?', '#']).next()?;
    let auth = auth.rsplit_once('@').map(|x| x.1).unwrap_or(auth);
    let h = if auth.starts_with('[') { auth.split(']').next().map(|x| format!("{x}]"))? } else { auth.split(':').next()?.to_string() };
    let h = h.to_ascii_lowercase();
    Some(h.strip_prefix("www.").map(String::from).unwrap_or(h)).filter(|h| !h.is_empty())
}

/// Does `host` equal `pattern` or end with `.pattern`? Patterns may be written as URLs.
pub fn host_matches(host: &str, pattern: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    let p = self::host(&p).unwrap_or(p);
    let p = p.trim_start_matches("*.").trim_end_matches('/');
    let p = p.strip_prefix("www.").unwrap_or(p);
    !p.is_empty() && (host == p || host.ends_with(&format!(".{p}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: &str = "https://s.test/?q=%s";
    #[test]
    fn inputs() {
        assert_eq!(resolve_input("example.com", T), "https://example.com");
        assert_eq!(resolve_input("example.com/a b", T), "https://s.test/?q=example.com%2Fa+b");
        assert_eq!(resolve_input("localhost:8080/x", T), "http://localhost:8080/x");
        assert_eq!(resolve_input("192.168.1.1", T), "http://192.168.1.1");
        assert_eq!(resolve_input("https://x.y/z", T), "https://x.y/z");
        assert_eq!(resolve_input("rust borrow checker", T), "https://s.test/?q=rust+borrow+checker");
        assert_eq!(resolve_input("file.txt", T), "https://file.txt"); // .txt looks like a TLD; acceptable
        assert_eq!(resolve_input("what?", T), "https://s.test/?q=what%3F");
        assert_eq!(resolve_input("c++", T), "https://s.test/?q=c%2B%2B");
        assert_eq!(resolve_input("", T), "emerald://start/");
        assert_eq!(resolve_input("emerald://settings", T), "emerald://settings");
    }
    #[test]
    fn hosts() {
        assert_eq!(host("https://www.YouTube.com/watch?v=1").as_deref(), Some("youtube.com"));
        assert_eq!(host("http://user:pw@m.reddit.com:443/r").as_deref(), Some("m.reddit.com"));
        assert!(host_matches("m.reddit.com", "reddit.com"));
        assert!(host_matches("reddit.com", "https://www.reddit.com/"));
        assert!(!host_matches("notreddit.com", "reddit.com"));
        assert!(!host_matches("reddit.com", ""));
    }
    #[test]
    fn query() {
        assert_eq!(parse_query("url=https%3A%2F%2Fa.b%2F&n=a+b&bad=%zz"), vec![("url".into(), "https://a.b/".into()), ("n".into(), "a b".into()), ("bad".into(), "%zz".into())]);
    }
}
