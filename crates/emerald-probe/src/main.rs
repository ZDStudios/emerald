//! emerald-probe: a tiny CDP client used for automated testing and benchmarks.
//!
//! ```text
//! emerald-probe targets                       list DevTools targets
//! emerald-probe eval  <match> <js>            evaluate JS in the first target whose url/title contains <match>
//! emerald-probe shot  <match> <out.png>       screenshot that target
//! emerald-probe nav   <match> <url>           navigate that target
//! emerald-probe mem   <pid>                   RSS + PSS of a process tree (Linux /proc)
//! emerald-probe wait  <match> <seconds>       wait until a target matching <match> exists
//! ```
//! Port defaults to 9222; override with EMERALD_CDP_PORT.
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

fn port() -> u16 {
    std::env::var("EMERALD_CDP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(9222)
}

fn http_get(path: &str) -> Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port())).context("connect to CDP port")?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", port())?;
    // DevTools' HTTP server may ignore `Connection: close`, so read by Content-Length.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16384];
    loop {
        let n = match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if !buf.is_empty() && matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => break,
            Err(e) => return Err(e.into()),
        };
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let len = head.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<usize>().ok()).flatten()
            });
            if len.is_some_and(|len| body.len() >= len) { break; }
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    Ok(text.split_once("\r\n\r\n").map(|x| x.1.to_string()).unwrap_or_default())
}

fn targets() -> Result<Vec<Value>> {
    let body = http_get("/json/list")?;
    let start = body.find('[').ok_or_else(|| anyhow!("bad /json body: {body}"))?;
    let end = body.rfind(']').unwrap_or(body.len() - 1);
    Ok(serde_json::from_str(&body[start..=end])?)
}

fn find(m: &str) -> Result<Value> {
    targets()?
        .into_iter()
        .filter(|t| t["type"] == "page" || t["type"] == "other" || t["type"] == "background_page" || t["type"] == "service_worker")
        .find(|t| {
            t["url"].as_str().unwrap_or("").contains(m) || t["title"].as_str().unwrap_or("").contains(m) || t["id"] == m
        })
        .ok_or_else(|| anyhow!("no target matching {m:?}"))
}

struct Cdp {
    ws: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    id: u64,
}

impl Cdp {
    fn open(t: &Value) -> Result<Self> {
        let url = t["webSocketDebuggerUrl"].as_str().ok_or_else(|| anyhow!("target has no ws url"))?;
        let (ws, _) = tungstenite::connect(url)?;
        Ok(Self { ws, id: 0 })
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let id = self.id;
        self.ws.send(tungstenite::Message::text(json!({"id": id, "method": method, "params": params}).to_string()))?;
        loop {
            let msg = self.ws.read()?;
            if let tungstenite::Message::Text(t) = msg {
                let v: Value = serde_json::from_str(&t)?;
                if v["id"] == id {
                    if let Some(e) = v.get("error") { bail!("{method}: {e}"); }
                    return Ok(v["result"].clone());
                }
            }
        }
    }
}

/// Walk /proc to collect a pid and all of its descendants.
fn tree(root: u32) -> Vec<u32> {
    let mut all: Vec<(u32, u32)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
            // ppid is the 2nd field after the parenthesised comm.
            let after = stat.rsplit_once(')').map(|x| x.1).unwrap_or("");
            let ppid = after.split_whitespace().nth(1).and_then(|p| p.parse().ok()).unwrap_or(0);
            all.push((pid, ppid));
        }
    }
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        let p = out[i];
        out.extend(all.iter().filter(|(_, pp)| *pp == p).map(|(c, _)| *c));
        i += 1;
    }
    out
}

fn kb(file: &str, key: &str) -> u64 {
    std::fs::read_to_string(file).ok().and_then(|s| {
        s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
    }).unwrap_or(0)
}

fn mem(root: u32) -> Value {
    let pids = tree(root);
    let (mut rss, mut pss) = (0u64, 0u64);
    let mut procs = Vec::new();
    for p in &pids {
        let r = kb(&format!("/proc/{p}/status"), "VmRSS:");
        let s = kb(&format!("/proc/{p}/smaps_rollup"), "Pss:");
        let cmd = std::fs::read(format!("/proc/{p}/cmdline")).unwrap_or_default();
        let ty = String::from_utf8_lossy(&cmd).split('\0').find_map(|a| a.strip_prefix("--type=").map(String::from)).unwrap_or_else(|| "browser".into());
        rss += r; pss += s;
        procs.push(json!({"pid": p, "type": ty, "rss_mb": r as f64 / 1024.0, "pss_mb": s as f64 / 1024.0}));
    }
    json!({"processes": pids.len(), "rss_mb": (rss as f64 / 1024.0).round(), "pss_mb": (pss as f64 / 1024.0).round(), "detail": procs})
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| a.get(i).cloned().ok_or_else(|| anyhow!("missing argument {i}; see source header for usage"));
    match arg(0)?.as_str() {
        "targets" => {
            for t in targets()? {
                println!("{:<16} {:<60} {}", t["type"].as_str().unwrap_or(""), t["title"].as_str().unwrap_or("").chars().take(60).collect::<String>(), t["url"].as_str().unwrap_or(""));
            }
        }
        "eval" => {
            let mut c = Cdp::open(&find(&arg(1)?)?)?;
            let r = c.call("Runtime.evaluate", json!({"expression": arg(2)?, "returnByValue": true, "awaitPromise": true, "userGesture": true}))?;
            println!("{}", serde_json::to_string_pretty(&r["result"].get("value").cloned().unwrap_or(r))?);
        }
        "shot" => {
            let mut c = Cdp::open(&find(&arg(1)?)?)?;
            let r = c.call("Page.captureScreenshot", json!({"format": "png"}))?;
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.decode(r["data"].as_str().unwrap_or(""))?;
            std::fs::write(arg(2)?, png)?;
            println!("saved {}", arg(2)?);
        }
        "nav" => {
            let mut c = Cdp::open(&find(&arg(1)?)?)?;
            c.call("Page.navigate", json!({"url": arg(2)?}))?;
        }
        "wait" => {
            let secs: u64 = arg(2)?.parse()?;
            let t0 = Instant::now();
            loop {
                if find(&arg(1)?).is_ok() { println!("ok after {:.1}s", t0.elapsed().as_secs_f64()); break; }
                if t0.elapsed().as_secs() > secs { bail!("timeout"); }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        "mem" => println!("{}", serde_json::to_string_pretty(&mem(arg(1)?.parse()?))?),
        other => bail!("unknown command {other}"),
    }
    Ok(())
}
