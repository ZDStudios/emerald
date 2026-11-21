// Settings page. Each control writes a JSON merge-patch through `settings.set`.
import { call, on, h, $, $$, icon, liveTheme, debounce, hostOf } from "./lib.js";

let S = null;      // settings
let themes = [];
const main = $("#main");

const get = (path) => path.split(".").reduce((o, k) => (o == null ? o : o[k]), S);
function patchFor(path, value) {
  const keys = path.split(".");
  const root = {};
  let o = root;
  keys.slice(0, -1).forEach((k) => (o = o[k] = {}));
  o[keys.at(-1)] = value;
  return root;
}
let savedTimer;
async function set(path, value) {
  S = await call("settings.set", { patch: patchFor(path, value) });
  const el = $(".saved");
  el.classList.add("on");
  clearTimeout(savedTimer);
  savedTimer = setTimeout(() => el.classList.remove("on"), 1200);
}

// ---------------------------------------------------------------- controls
const row = (label, desc, ctl) => h("div.row", h("div.grow", h("div.label", label), desc ? h("div.desc", desc) : null), h("div.ctl", ctl));
const toggle = (path) => { const c = h("input.switch", { type: "checkbox", checked: !!get(path), "aria-label": path }); c.onchange = () => set(path, c.checked); return c; };
const num = (path, min, max, suffix) => {
  const i = h("input.input", { type: "number", min, max, value: get(path) });
  i.onchange = () => set(path, Math.max(min, Math.min(max, Number(i.value) || 0)));
  return [i, suffix ? h("span.muted", suffix) : null];
};
const select = (path, opts) => {
  const s = h("select.input", ...opts.map(([v, t]) => h("option", { value: v, selected: String(get(path)) === String(v) }, t)));
  s.onchange = () => set(path, s.value);
  return s;
};
const seg = (path, opts) => {
  const w = h("div.seg");
  for (const [v, t] of opts) {
    const b = h("button", { onclick: async () => { await set(path, v); for (const x of w.children) x.classList.toggle("on", x === b); } }, t);
    if (get(path) === v) b.classList.add("on");
    w.append(b);
  }
  return w;
};
const group = (id, title, desc, ...rows) => h("section.setting-group", { id }, h("h2", title), desc ? h("p", desc) : null, h("div.card", ...rows));

// ---------------------------------------------------------------- sections
function appearance() {
  const grid = h("div.theme-grid", ...themes.map((t) => {
    const c = t.colors;
    const card = h("button.theme-card", { onclick: () => set("theme", t.id).then(render) },
      h("div.pv", h("div.sb", { style: { background: c.surface } }),
        h("div.ct", { style: { background: c.bg } }, h("div.ln", { style: { background: c.accent, width: "50%" } }), h("div.ln", { style: { background: c["text-muted"], width: "80%", opacity: .5 } }), h("div.ln", { style: { background: c["text-muted"], width: "65%", opacity: .5 } }))),
      h("div.nm", t.name, t.builtin ? "" : " ✦"));
    if (S.theme === t.id) card.classList.add("on");
    return card;
  }));
  const css = h("textarea.input.code", { spellcheck: "false", placeholder: "/* CSS for Emerald's own interface. Example: */\n.tab { border-radius: 20px; }\n#topbar { background: linear-gradient(90deg, var(--em-bg), var(--em-surface)); }" }, S.custom_css);
  css.oninput = debounce(() => set("custom_css", css.value), 500);
  return group("appearance", "Appearance", "Themes, layout and density. Changes apply instantly.",
    grid,
    h("div.row", h("div.grow", h("div.label", "Theme editor"), h("div.desc", "Make your own theme, tweak colors and fonts live, import or export theme packs.")), h("a.btn", { href: "emerald://themes/" }, h("span", { html: icon("palette", 15) }), "Open editor")),
    row("Tab layout", "Vertical sidebar or classic horizontal tabs.", seg("layout.tabs", [["vertical", "Vertical"], ["horizontal", "Horizontal"]])),
    row("Density", null, seg("layout.density", [["compact", "Compact"], ["comfortable", "Comfortable"], ["spacious", "Spacious"]])),
    row("Sidebar width", null, num("layout.sidebar_width", 180, 420, "px")),
    row("Sidebar on the right", null, toggle("layout.sidebar_right")),
    row("Auto-hide sidebar", "Move the pointer to the window edge to reveal it.", toggle("layout.sidebar_autohide")),
    row("Auto-hide top bar", "Move the pointer to the top edge to reveal it.", toggle("layout.topbar_autohide")),
    row("Show window buttons", "Minimize / maximize / close in the top bar.", toggle("layout.show_window_controls")),
    h("div.row", h("div.grow", h("div.label", "Custom CSS"), h("div.desc", "Injected into Emerald's interface (not web pages). Use the --em-* variables from your theme."), css)),
  );
}

function startPage() {
  const bg = h("input.input", { value: S.start_page.background, placeholder: "e.g. #10201a or linear-gradient(…) (empty = theme default)" });
  bg.onchange = () => set("start_page.background", bg.value);
  return group("start", "Start page", "Your new-tab page. Use “Customize” on the page to drag and resize widgets.",
    row("Greeting", null, toggle("start_page.greeting")),
    row("Background", "Any CSS background.", bg),
    row("Search engine", null, select("search.engine", [["DuckDuckGo", "DuckDuckGo"], ["Google", "Google"], ["Bing", "Bing"], ["Brave", "Brave Search"], ["Startpage", "Startpage"], ["Ecosia", "Ecosia"]])),
  );
}

function adhd() {
  const blocked = h("div.chips", ...S.adhd.blocked_sites.map((site) => h("span.pill", site, h("button", { title: "Remove", html: icon("close", 12), onclick: () => set("adhd.blocked_sites", S.adhd.blocked_sites.filter((x) => x !== site)).then(render) }))));
  const add = h("input.input", { placeholder: "Add a site, e.g. news.ycombinator.com", style: { maxWidth: "320px" } });
  add.onkeydown = (e) => { if (e.key === "Enter" && add.value.trim()) { const v = hostOf(add.value.includes("://") ? add.value : "https://" + add.value) || add.value.trim(); set("adhd.blocked_sites", [...new Set([...S.adhd.blocked_sites, v])]).then(render); } };
  blocked.append(add);
  const DAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
  const t2s = (m) => `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
  const s2t = (s) => { const [a, b] = s.split(":").map(Number); return (a || 0) * 60 + (b || 0); };
  const scheds = S.adhd.schedules.map((sc, i) => {
    const upd = (patch) => set("adhd.schedules", S.adhd.schedules.map((x, j) => (i === j ? { ...x, ...patch } : x))).then(render);
    const on = h("input.switch", { type: "checkbox", checked: sc.enabled, onchange: (e) => upd({ enabled: e.target.checked }) });
    const nm = h("input.input", { value: sc.name, style: { width: "140px" }, onchange: (e) => upd({ name: e.target.value }) });
    const from = h("input.input", { type: "time", value: t2s(sc.start_min), style: { width: "110px" }, onchange: (e) => upd({ start_min: s2t(e.target.value) }) });
    const to = h("input.input", { type: "time", value: t2s(sc.end_min), style: { width: "110px" }, onchange: (e) => upd({ end_min: s2t(e.target.value) }) });
    const days = h("div.days", ...DAYS.map((d, j) => { const b = h("button", { onclick: () => upd({ days: sc.days.map((x, k) => (k === j ? !x : x)) }) }, d); if (sc.days[j]) b.classList.add("on"); return b; }));
    return h("div.sched", on, nm, days, from, h("span.muted", "to"), to, h("button.iconbtn", { title: "Delete schedule", html: icon("trash", 15), onclick: () => set("adhd.schedules", S.adhd.schedules.filter((_, j) => j !== i)).then(render) }));
  });
  return group("focus", "Focus & ADHD", "Calm by default. Every feature here is optional, so turn on only what helps you.",
    row("ADHD-friendly features", "Site blocker, focus schedules, nudges and the timer.", toggle("adhd.enabled")),
    row("Quiet mode", "No pop-ups or notification prompts. Sites asking to send notifications are declined silently.", toggle("adhd.quiet")),
    row("Reduce motion", "Turns off animations and transitions everywhere in Emerald.", toggle("adhd.reduced_motion")),
    row("Dyslexia-friendly font", "Uses OpenDyslexic for Emerald's interface and reader mode.", toggle("adhd.dyslexia_font")),
    row("Big click targets", "Larger buttons and rows.", toggle("adhd.big_targets")),
    row("Tab limit nudge", "A gentle reminder when a space has more tabs than this (0 = off).", num("adhd.tab_limit", 0, 200, "tabs")),
    row("Distraction nudge", "After this long on a distracting site outside focus time, ask if it's what you meant to do (0 = off).", num("adhd.distraction_nudge_min", 0, 240, "min")),
    h("div.row", h("div.grow", h("div.label", "Distracting sites"), h("div.desc", "Blocked during focus sessions, Pomodoro work blocks and scheduled focus time. You can always allow one for 5 minutes."))),
    blocked,
    h("div.row", h("div.grow", h("div.label", "Scheduled focus time"), h("div.desc", "Distracting sites are blocked automatically during these hours.")),
      h("button.btn.small", { onclick: () => set("adhd.schedules", [...S.adhd.schedules, { name: "Focus", days: [true, true, true, true, true, false, false], start_min: 540, end_min: 720, enabled: true }]).then(render) }, h("span", { html: icon("plus", 14) }), "Add")),
    ...scheds,
    row("Pomodoro: focus block", null, num("adhd.pomodoro.work_min", 5, 120, "min")),
    row("Pomodoro: short break", null, num("adhd.pomodoro.short_break_min", 1, 60, "min")),
    row("Pomodoro: long break", null, num("adhd.pomodoro.long_break_min", 1, 90, "min")),
    row("Long break after", null, num("adhd.pomodoro.cycles_before_long", 1, 12, "blocks")),
    row("Block distracting sites during focus blocks", null, toggle("adhd.pomodoro.block_during_work")),
  );
}

function memory() {
  return group("memory", "Memory", "Chromium can't use zero RAM, but Emerald keeps it as low as it can.",
    row("Put background tabs to sleep after", "Sleeping tabs keep their place but use no memory. 0 = never.", num("memory.sleep_after_minutes", 0, 1440, "min")),
    row("Lazy restore", "After a restart, only the tab you're looking at is loaded.", toggle("memory.lazy_restore")),
    row("Never sleep pinned tabs", null, toggle("memory.never_sleep_pinned")),
    row("Never sleep tabs playing audio", null, toggle("memory.never_sleep_audible")),
    row("Renderer process limit", "Caps the number of page processes (0 = Chromium default). Applies after restart.", num("memory.renderer_process_limit", 0, 64)),
    h("div.row", h("div.grow", h("div.label", "Free memory now"), h("div.desc", "Puts every tab you're not looking at to sleep.")), h("button.btn", { onclick: async () => { const n = await call("memory.free"); alertish(`${n} tab${n === 1 ? "" : "s"} put to sleep`); } }, h("span", { html: icon("moon", 15) }), "Free memory")),
  );
}

function privacy() {
  const perms = h("div");
  call("permissions.list").then((p) => {
    const sites = Object.entries(p || {});
    perms.replaceChildren(...(sites.length ? sites.map(([origin, bits]) => h("div.row", h("div.grow", h("div.label", origin), h("div.desc", Object.entries(bits).map(([b, a]) => `${a ? "Allowed" : "Blocked"} ${Number(b) >= 1 << 26 ? "camera/mic" : "permission " + Math.log2(Number(b))}`).join(" · "))),
      h("button.btn.small.ghost", { onclick: async () => { for (const b of Object.keys(bits)) await call("permissions.set", { origin, perms: Number(b), allow: null }); render(); } }, "Reset")))
      : [h("div.row", h("div.desc", "No site permissions saved yet."))]));
  });
  return group("privacy", "Privacy & security", "No telemetry. Nothing leaves your computer unless a page you visit requests it.",
    row("Block ads and trackers", "Emerald's built-in, lightweight blocker. For full filter lists, add uBlock Origin Lite.", toggle("privacy.block_ads_trackers")),
    row("HTTPS-First", "Upgrade connections to HTTPS and warn before loading insecure sites.", toggle("privacy.https_first")),
    row("Block third-party cookies", null, toggle("privacy.block_third_party_cookies")),
    row("Send “Do Not Track” and Global Privacy Control", null, toggle("privacy.send_do_not_track")),
    h("div.row", h("div.grow", h("div.label", "Clear browsing history")), h("button.btn.small", { onclick: () => call("history.clear", { since: Date.now() - 3600e3 }).then(() => alertish("Last hour cleared")) }, "Last hour"), h("button.btn.small", { onclick: () => call("history.clear", { since: 0 }).then(() => alertish("History cleared")) }, "All time")),
    h("div.row", h("div.grow", h("div.label", "Site permissions"), h("div.desc", "Choices you made for camera, microphone, location, notifications and so on."))),
    perms,
  );
}

function general() {
  const dl = h("input.input", { value: S.downloads_dir || "", placeholder: "Default: your Downloads folder" });
  dl.onchange = () => set("downloads_dir", dl.value.trim() || null);
  return group("general", "General", null,
    row("Restore tabs on startup", null, toggle("startup.restore_session")),
    row("Downloads folder", null, dl),
    h("div.row", h("div.grow", h("div.label", "Import from Chrome or Edge"), h("div.desc", "Bookmarks, history and passwords, plus HTML / CSV files.")), h("a.btn", { href: "emerald://welcome/#import" }, h("span", { html: icon("import", 15) }), "Import")),
    h("div.row", h("div.grow", h("div.label", "Extensions"), h("div.desc", "Install from the Chrome Web Store or Edge Add-ons with “Add to Emerald”.")), h("a.btn", { href: "emerald://extensions/" }, h("span", { html: icon("puzzle", 15) }), "Manage")),
    h("div.row", h("div.grow", h("div.label", "Setup wizard"), h("div.desc", "Run the first-run setup again.")), h("a.btn", { href: "emerald://welcome/" }, "Open")),
  );
}

function about() {
  return group("about", "About", null,
    h("div.row", h("img", { src: "img/emerald-64.png", width: 40, height: 40, alt: "" }), h("div.grow", h("div.label", "Emerald 0.1.0"), h("div.desc", navigator.userAgent.match(/Chrome\/[\d.]+/)?.[0]?.replace("Chrome", "Chromium engine") + " via CEF · Rust shell · no telemetry"))),
  );
}

function alertish(text) { const el = $(".saved"); el.textContent = text; el.classList.add("on"); setTimeout(() => { el.classList.remove("on"); el.textContent = "Saved"; }, 1800); }

const SECTIONS = [["appearance", "Appearance", "palette", appearance], ["start", "Start page", "home", startPage], ["focus", "Focus & ADHD", "focus", adhd], ["memory", "Memory", "chip", memory], ["privacy", "Privacy", "shield", privacy], ["general", "General", "settings", general], ["about", "About", "leaf", about]];

function render() {
  const y = window.scrollY;
  $("#nav").replaceChildren(h("div.brand", h("img", { src: "img/emerald-64.png", alt: "" }), "Settings"),
    ...SECTIONS.map(([id, t, ic]) => h("a", { href: "#" + id }, h("span", { html: icon(ic, 16) }), t)));
  main.replaceChildren(h("h1", "Settings"), h("p.lead", "Everything is saved as you go."), ...SECTIONS.map((s) => s[3]()), h("div.pill.accent.saved", "Saved"));
  window.scrollTo(0, y);
  if (location.hash && !render.scrolled) { render.scrolled = true; document.getElementById(location.hash.slice(1))?.scrollIntoView(); }
}

window.addEventListener("scroll", () => {
  let cur = SECTIONS[0][0];
  for (const [id] of SECTIONS) { const el = document.getElementById(id); if (el && el.getBoundingClientRect().top < 120) cur = id; }
  for (const a of $$("#nav a")) a.classList.toggle("on", a.getAttribute("href") === "#" + cur);
});

liveTheme();
on("state", (st) => { if (JSON.stringify(st.settings) !== JSON.stringify(S)) { S = st.settings; } });
Promise.all([call("settings.get"), call("themes.list")]).then(([s, t]) => { S = s; themes = t; render(); });
