// First-run setup wizard: look, layout, focus features, import, accounts.
import { call, h, $, icon, liveTheme } from "./lib.js";

let S = null, themes = [], profiles = [];
let step = 0;
const main = $("#main");
main.className = "wizard";

const set = async (patch) => { S = await call("settings.set", { patch }); };
const STEPS = ["Welcome", "Look", "Focus", "Import", "Accounts", "Ready"];

function frame(title, lead, body, { next = "Continue", back = true, onNext } = {}) {
  return h("div.wz",
    h("div.wz-steps", ...STEPS.map((s, i) => h("span" + (i === step ? ".on" : i < step ? ".done" : ""), s))),
    h("h1", title), lead ? h("p.lead", lead) : null,
    h("div.wz-body", body),
    h("div.wz-nav",
      back && step > 0 ? h("button.btn.ghost", { onclick: () => go(step - 1) }, "Back") : h("span"),
      h("span", { style: { flex: 1 } }),
      step < STEPS.length - 1 ? h("button.btn.ghost", { onclick: () => go(STEPS.length - 1) }, "Skip setup") : null,
      h("button.btn.primary", { onclick: async () => { if (onNext) await onNext(); go(step + 1); } }, next)));
}

function welcome() {
  return frame("Welcome to Emerald", "A calm, minimal browser that keeps out of your way. Let's set it up in about a minute. You can change everything later.",
    h("div.hero", h("img", { src: "img/emerald-256.png", width: 120, height: 120, alt: "" }),
      h("ul.feat",
        h("li", h("span", { html: icon("leaf", 18) }), "Minimal: thin bars that can hide themselves."),
        h("li", h("span", { html: icon("focus", 18) }), "Focus tools: blocker, timer, parking, quick notes."),
        h("li", h("span", { html: icon("moon", 18) }), "Light on memory: idle tabs go to sleep."),
        h("li", h("span", { html: icon("shield", 18) }), "Private: built-in tracker blocking, no telemetry."))),
    { next: "Let's go", back: false });
}

function look() {
  const cards = h("div.theme-grid", ...themes.map((t) => {
    const c = t.colors;
    const b = h("button.theme-card", { onclick: async () => { await set({ theme: t.id }); render(); } },
      h("div.pv", h("div.sb", { style: { background: c.surface } }), h("div.ct", { style: { background: c.bg } }, h("div.ln", { style: { background: c.accent, width: "55%" } }), h("div.ln", { style: { background: c["text-muted"], width: "80%", opacity: .5 } }))),
      h("div.nm", t.name));
    if (S.theme === t.id) b.classList.add("on");
    return b;
  }));
  const opt = (path, val, title, desc, ic) => {
    const cur = path.split(".").reduce((o, k) => o[k], S);
    const b = h("button.choice", { onclick: async () => { await set(path === "layout.tabs" ? { layout: { tabs: val } } : { layout: { density: val } }); render(); } }, h("span", { html: icon(ic, 22) }), h("b", title), h("small", desc));
    if (cur === val) b.classList.add("on");
    return b;
  };
  return frame("Make it yours", "Pick a theme and a layout. Changes apply instantly.", h("div",
    cards,
    h("h3", "Tabs"),
    h("div.choices3", opt("layout.tabs", "vertical", "Vertical", "Sidebar list: great with many tabs", "sidebar"), opt("layout.tabs", "horizontal", "Horizontal", "Classic tab strip on top", "layout")),
    h("h3", "Density"),
    h("div.choices3", opt("layout.density", "compact", "Compact", "More on screen", "min"), opt("layout.density", "comfortable", "Comfortable", "Balanced", "grid"), opt("layout.density", "spacious", "Spacious", "Roomy and calm", "max"))));
}

function focusStep() {
  const tog = (path, label, desc) => {
    const cur = path.split(".").reduce((o, k) => o[k], S);
    const keys = path.split(".");
    const sw = h("input.switch", { type: "checkbox", checked: !!cur, onchange: (e) => { const p = {}; let o = p; keys.slice(0, -1).forEach((k) => (o = o[k] = {})); o[keys.at(-1)] = e.target.checked; set(p); } });
    return h("label.row", h("div.grow", h("div.label", label), h("div.desc", desc)), sw);
  };
  const limit = h("input.input", { type: "number", min: 0, max: 100, value: S.adhd.tab_limit, style: { width: "80px" }, onchange: (e) => set({ adhd: { tab_limit: Number(e.target.value) || 0 } }) });
  return frame("Focus features", "Built for ADHD brains, and helpful for anyone. All optional, all quiet.", h("div.card",
    tog("adhd.enabled", "Focus tools", "Pomodoro timer, focus sessions and a distraction blocker for the sites you pick."),
    tog("adhd.quiet", "Quiet by default", "No notification prompts, no pop-up spam. Emerald only speaks up when you asked it to."),
    tog("adhd.reduced_motion", "Reduce motion", "No animations or sliding."),
    tog("adhd.dyslexia_font", "Dyslexia-friendly font", "OpenDyslexic for Emerald's interface and reader mode."),
    tog("adhd.big_targets", "Bigger click targets", "Larger buttons and rows."),
    h("label.row", h("div.grow", h("div.label", "Gentle tab-limit nudge"), h("div.desc", "Remind me to park or close tabs above this many (0 = off).")), limit),
    h("div.row", h("div.grow", h("div.label", "Distracting sites"), h("div.desc", S.adhd.blocked_sites.join(", ") + ". Edit the list any time in Settings → Focus.")))));
}

function importStep() {
  const results = h("div.import-results");
  const rows = profiles.length ? profiles.map((p) => {
    const cb = (k, label, avail) => h("label.ck", h("input", { type: "checkbox", checked: avail && k !== "history", disabled: !avail, dataset: { k } }), label);
    const row = h("div.row", h("div.grow", h("div.label", `${p.browser}: ${p.name}`), h("div.desc", p.dir)),
      cb("bookmarks", "Bookmarks", p.bookmarks), cb("history", "History", p.history), cb("passwords", "Passwords", p.passwords),
      h("button.btn.small", { onclick: async (e) => {
        const btn = e.currentTarget;
        const what = Object.fromEntries([...row.querySelectorAll("input[data-k]")].map((i) => [i.dataset.k, i.checked]));
        btn.disabled = true; btn.textContent = "Importing…";
        try { show(await call("import.run", { dir: p.dir, browser: p.browser, ...what }), `${p.browser} (${p.name})`); }
        catch (err) { show({ error: err.message }); }
        btn.disabled = false; btn.textContent = "Import again";
      } }, "Import"));
    return row;
  }) : [h("div.row", h("div.desc", "No Chrome or Edge profiles found on this computer. You can import exported files below."))];
  const show = (r, from) => {
    const lines = [];
    if (r.error) lines.push(h("div.err", r.error));
    for (const [k, v] of Object.entries(r)) {
      if (k === "error") continue;
      lines.push(h("div", v.error ? h("span.err", `${k}: ${v.error}`) : `${k[0].toUpperCase() + k.slice(1)}: ${v.added} imported${v.skipped ? `, ${v.skipped} couldn't be read` : ""}.`));
      if (v.reason) lines.push(h("div.note", v.reason));
    }
    results.prepend(h("div.card.res", from ? h("b", from) : null, ...lines));
  };
  const file = (kind, accept, label) => {
    const input = h("input", { type: "file", accept, hidden: true, onchange: async () => {
      const f = input.files[0];
      if (!f) return;
      try { show(await call("import.file", { kind, text: await f.text() }), f.name); } catch (err) { show({ error: err.message }); }
      input.value = "";
    } });
    return h("button.btn", { onclick: () => input.click() }, h("span", { html: icon("import", 15) }), label, input);
  };
  return frame("Bring your stuff", "Emerald reads Chrome and Edge profile folders directly. Nothing is uploaded anywhere.", h("div",
    h("div.card", ...rows),
    h("h3", "Or import a file"),
    h("div", { style: { display: "flex", gap: "10px", flexWrap: "wrap" } }, file("bookmarks-html", ".html,.htm", "Bookmarks (HTML)"), file("passwords-csv", ".csv", "Passwords (CSV)")),
    h("p.note", "About passwords: Chrome 127 and later locks saved passwords with “app-bound encryption” that only Chrome itself can unlock. If Chrome passwords show as skipped, export them from Chrome (Settings → Passwords → ⋮ → Export passwords) and import the CSV here. Delete the CSV afterwards, because it isn't encrypted."),
    results));
}

function accounts() {
  return frame("Sign in to your accounts", "Use Google and Microsoft services (Gmail, Drive, YouTube, Outlook, OneDrive) in Emerald as in any browser.", h("div",
    h("div.choices3",
      h("button.choice", { onclick: () => call("account.signin", { provider: "google" }) }, h("span", { html: icon("user", 22) }), h("b", "Sign in to Google"), h("small", "Opens Google's sign-in page in a tab")),
      h("button.choice", { onclick: () => call("account.signin", { provider: "microsoft" }) }, h("span", { html: icon("user", 22) }), h("b", "Sign in to Microsoft"), h("small", "Opens Microsoft's sign-in page in a tab"))),
    h("div.card", { style: { padding: "14px 16px", marginTop: "16px" } },
      h("b", "What signing in can't do"),
      h("p.note", "Google only offers Chrome Sync to Chrome, and Microsoft only offers Edge sync to Edge. No other browser can sync bookmarks, passwords or open tabs through those accounts. Emerald's honest alternatives: import once (previous step), and use a password-manager extension such as Bitwarden to keep passwords in sync across browsers and devices."),
      h("p.note", "Emerald identifies itself as a standard Chromium browser, so Google's “This browser may not be secure” check shouldn't trigger. If it ever does, sign in from the Google tab after allowing third-party cookies for accounts.google.com, or use a passkey."))));
}

function ready() {
  const k = (keys, what) => h("div.tip", h("span", ...keys.split(" ").map((x) => h("span.kbd", x))), h("span", what));
  return frame("You're all set", "A few shortcuts worth remembering:", h("div.tips",
    k("Ctrl K", "Command bar: tabs, history, commands, URLs"), k("Ctrl T", "New tab"), k("Ctrl Shift F", "Focus mode: just this page"),
    k("Ctrl Shift P", "Park a tab for later, with a reminder"), k("Ctrl Shift N", "Quick note"), k("Ctrl Shift E", "Split view"),
    k("Ctrl Shift R", "Reader mode"), k("Ctrl Shift M", "Memory meter")),
    { next: "Start browsing", onNext: async () => { await call("wizard.done"); location.href = "emerald://start/"; } });
}

const VIEWS = [welcome, look, focusStep, importStep, accounts, ready];
function go(i) { step = Math.max(0, Math.min(VIEWS.length - 1, i)); render(); window.scrollTo(0, 0); }
function render() { main.replaceChildren(VIEWS[step]()); }

const style = h("style", `
  .wizard { max-width: 760px; margin: 0 auto; padding: 48px 28px 80px; }
  .wz-steps { display: flex; gap: 6px; margin-bottom: 30px; flex-wrap: wrap; }
  .wz-steps span { font-size: 12px; padding: 3px 10px; border-radius: 99px; color: var(--em-text-muted); border: 1px solid var(--em-border); }
  .wz-steps span.on { background: var(--em-accent); color: var(--em-accent-text); border-color: transparent; }
  .wz-steps span.done { color: var(--em-accent); border-color: var(--em-accent-soft); }
  .wizard h1 { font-size: 28px; margin: 0 0 8px; font-weight: 650; }
  .wizard h3 { margin: 22px 0 10px; font-size: 13px; text-transform: uppercase; letter-spacing: .06em; color: var(--em-text-muted); }
  .wz-body { margin: 22px 0 30px; }
  .wz-nav { display: flex; gap: 10px; align-items: center; }
  .hero { display: flex; gap: 32px; align-items: center; }
  .feat { list-style: none; padding: 0; margin: 0; display: grid; gap: 14px; font-size: 15px; }
  .feat li { display: flex; gap: 12px; align-items: center; } .feat li span { color: var(--em-accent); }
  .choices3 { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 10px; }
  .choice { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 14px; border: 2px solid var(--em-border); border-radius: var(--em-radius); background: var(--em-surface); cursor: pointer; text-align: left; }
  .choice span { color: var(--em-accent); margin-bottom: 4px; } .choice small { color: var(--em-text-muted); }
  .choice.on { border-color: var(--em-accent); background: var(--em-accent-soft); }
  .ck { display: flex; gap: 6px; align-items: center; font-size: 12.5px; color: var(--em-text-muted); }
  .note { color: var(--em-text-muted); font-size: 12.5px; line-height: 1.6; margin: 10px 0 0; }
  .res { padding: 12px 14px; margin-top: 12px; display: grid; gap: 4px; } .err { color: var(--em-danger); }
  .tips { display: grid; gap: 10px; } .tip { display: flex; gap: 14px; align-items: center; } .tip > span:first-child { display: flex; gap: 4px; min-width: 150px; }
  label.row { cursor: pointer; }
`);
document.head.append(style);
liveTheme();
Promise.all([call("settings.get"), call("themes.list"), call("import.detect")]).then(([s, t, p]) => {
  S = s; themes = t; profiles = p;
  if (location.hash === "#import") step = 3;
  render();
});
