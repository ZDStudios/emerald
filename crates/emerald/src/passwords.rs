//! Password saving and autofill backed by the encrypted vault (emerald_core::vault).
//!
//! Flow:
//! 1. After each http(s) page load, Emerald injects a small detector with a per-load random
//!    nonce. It reports "login form present" and, on submit, the typed credentials over a
//!    console channel tagged with the nonce.
//! 2. Emerald's own UI (never the page) offers "Fill" / "Save". Credentials enter a page
//!    only after the user clicks Fill in Emerald's top bar.
//!
//! A page could forge reports, but the worst it can do is make Emerald *ask* to save or
//! fill. Nothing happens without the user's click.
use crate::shell::Shell;
use crate::util;
use cef::*;
use emerald_core::session::Id;
use emerald_core::vault::{origin_of, Vault};
use serde_json::{json, Value};

pub const PREFIX: &str = "\u{1}EMPW:";

fn nonce() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn vault(s: &mut Shell) -> Option<&mut Vault> {
    if s.vault.is_none() {
        match Vault::open(&s.paths.data()) {
            Ok(v) => s.vault = Some(v),
            Err(e) => { eprintln!("emerald: vault unavailable: {e}"); return None; }
        }
    }
    s.vault.as_mut()
}

pub fn import(s: &mut Shell, items: impl Iterator<Item = (String, String, String)>) -> usize {
    let Some(v) = vault(s) else { return 0 };
    let n = items.filter(|(o, u, p)| v.upsert(o, u, p, "imported")).count();
    let _ = v.save();
    n
}

const DETECT_JS: &str = r#"(function(N){
  if (window.__emPw === N) return; window.__emPw = N;
  const send = (k, d) => console.log("\u0001EMPW:" + N + ":" + k + ":" + JSON.stringify(d || {}));
  const pw = () => [...document.querySelectorAll('input[type=password]')].filter(i => i.offsetParent !== null);
  const userFor = (p) => {
    const form = p.form || document;
    const ins = [...form.querySelectorAll('input')].filter(i => /^(text|email|tel|)$/.test(i.type) && i.offsetParent !== null);
    const before = ins.filter(i => i.compareDocumentPosition(p) & Node.DOCUMENT_POSITION_FOLLOWING);
    return before.pop() || ins[0] || null;
  };
  let reported = false;
  const check = () => { if (!reported && pw().length) { reported = true; send("form"); } };
  check();
  new MutationObserver(check).observe(document.documentElement, { childList: true, subtree: true });
  const capture = () => { const p = pw().find(i => i.value); if (!p) return; const u = userFor(p); send("save", { u: u ? u.value : "", p: p.value }); };
  document.addEventListener("submit", capture, true);
  document.addEventListener("click", (e) => { if (e.target.closest('button,[type=submit],[role=button]')) setTimeout(capture, 0); }, true);
})"#;

pub fn on_page_loaded(s: &mut Shell, tab: Id, b: &Browser, url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) { return; }
    let n = nonce();
    s.pw_nonce.insert(tab, n.clone());
    util::exec_js(b, &format!("{DETECT_JS}({})", serde_json::to_string(&n).unwrap()));
}

/// Handle a detector report from a page. Returns true if the console message was ours.
pub fn on_console(s: &mut Shell, tab: Id, msg: &str) -> bool {
    let Some(rest) = msg.strip_prefix(PREFIX) else { return false };
    let mut parts = rest.splitn(3, ':');
    let (Some(n), Some(kind), data) = (parts.next(), parts.next(), parts.next().unwrap_or("{}")) else { return true };
    if s.pw_nonce.get(&tab).map(String::as_str) != Some(n) { return true; }
    let Some(origin) = s.session.tab(tab).and_then(|t| origin_of(&t.url)) else { return true };
    match kind {
        "form" => {
            let Some(v) = vault(s) else { return true };
            let accounts: Vec<Value> = v.for_origin(&origin).iter().map(|c| json!({"id": c.id, "username": c.username})).collect();
            if !accounts.is_empty() { s.emit_ui("passwords", json!({"tab": tab, "origin": origin, "accounts": accounts})); }
        }
        "save" => {
            let d: Value = serde_json::from_str(data).unwrap_or_default();
            let (u, p) = (d["u"].as_str().unwrap_or("").to_string(), d["p"].as_str().unwrap_or("").to_string());
            if p.is_empty() || s.pw_never.contains(&origin) { return true; }
            let Some(v) = vault(s) else { return true };
            if v.for_origin(&origin).iter().any(|c| c.username == u && c.password == p) { return true; }
            if s.pw_pending.get(&tab) == Some(&(origin.clone(), u.clone(), p.clone())) { return true; }
            s.pw_pending.insert(tab, (origin.clone(), u.clone(), p));
            s.emit_ui("passwordSave", json!({"tab": tab, "origin": origin, "username": u}));
        }
        _ => {}
    }
    true
}

pub fn api(s: &mut Shell, cmd: &str, a: &Value) -> anyhow::Result<Value> {
    Ok(match cmd {
        "pw.fill" => {
            let tab = a["tab"].as_u64().unwrap_or(0);
            let id = a["id"].as_u64().unwrap_or(0);
            let Some(origin) = s.session.tab(tab).and_then(|t| origin_of(&t.url)) else { return Ok(Value::Null) };
            let cred = vault(s).and_then(|v| v.items.iter_mut().find(|c| c.id == id && c.origin == origin).map(|c| { c.used = emerald_core::now_ms(); (c.username.clone(), c.password.clone()) }));
            if let (Some((u, p)), Some(b)) = (cred, s.browser(tab)) {
                let js = format!(r#"(function(u,p){{
                  const set=(el,v)=>{{const d=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value');d.set.call(el,v);el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));}};
                  const ps=[...document.querySelectorAll('input[type=password]')].filter(i=>i.offsetParent!==null);
                  const p0=ps[0]; if(!p0) return;
                  const form=p0.form||document; const ins=[...form.querySelectorAll('input')].filter(i=>/^(text|email|tel|)$/.test(i.type)&&i.offsetParent!==null);
                  const before=ins.filter(i=>i.compareDocumentPosition(p0)&Node.DOCUMENT_POSITION_FOLLOWING); const uf=before.pop()||ins[0];
                  if(uf&&u) set(uf,u); set(p0,p); p0.focus();
                }})({},{})"#, serde_json::to_string(&u)?, serde_json::to_string(&p)?);
                util::exec_js(&b, &js);
                if let Some(v) = s.vault.as_ref() { let _ = v.save(); }
            }
            Value::Null
        }
        "pw.save" => {
            let tab = a["tab"].as_u64().unwrap_or(0);
            if let Some((o, u, p)) = s.pw_pending.remove(&tab) {
                if a["never"].as_bool() == Some(true) { s.pw_never.insert(o); }
                else if let Some(v) = vault(s) { v.upsert(&o, &u, &p, ""); let _ = v.save(); }
            }
            Value::Null
        }
        "pw.list" => {
            let v = vault(s).map(|v| v.items.iter().map(|c| json!({"id": c.id, "origin": c.origin, "username": c.username, "created": c.created, "used": c.used})).collect::<Vec<_>>()).unwrap_or_default();
            json!(v)
        }
        "pw.reveal" => {
            let id = a["id"].as_u64().unwrap_or(0);
            json!(vault(s).and_then(|v| v.items.iter().find(|c| c.id == id).map(|c| c.password.clone())))
        }
        "pw.delete" => {
            let id = a["id"].as_u64().unwrap_or(0);
            if let Some(v) = vault(s) { v.remove(id); let _ = v.save(); }
            Value::Null
        }
        _ => anyhow::bail!("unknown"),
    })
}
