// The overlay: the one Emerald surface that floats above web pages.
// Modes: palette (Ctrl+K / address bar), menu, park, note, memory.
import { call, on, h, $, icon, favicon, prettyUrl, fmtBytes, debounce, liveTheme } from "./lib.js";

const root = $("#root");
const close = () => call("overlay.close");
let mode = null;

// Commands available from the command bar. `act` is [api, args].
const C = (text, name, icn, key) => ({ text, act: ["cmd", { name }], icon: icn, key });
const COMMANDS = [
  C("New tab", "tab.new", "plus", "Ctrl T"), C("Close tab", "tab.close", "close", "Ctrl W"), C("Reopen closed tab", "tab.reopen", "reload", "Ctrl Shift T"),
  C("Pin / unpin tab", "tab.pin", "pin"), C("Mute / unmute tab", "tab.mute", "mute"), C("Park this tab for later", "tab.park", "park", "Ctrl Shift P"),
  C("Split view", "split.toggle", "split", "Ctrl Shift E"), C("Focus mode: hide everything but this tab", "focus.toggle", "focus", "Ctrl Shift F"),
  C("Toggle sidebar", "sidebar.toggle", "sidebar", "Ctrl Shift S"), C("Reader mode", "reader.toggle", "reader", "Ctrl Shift R"),
  C("Distraction-free video", "video.focus", "video"), C("Quick note", "note.quick", "note", "Ctrl Shift N"),
  C("Memory meter", "memory", "chip", "Ctrl Shift M"), C("Free memory: sleep background tabs", "memory.free", "moon"),
  { text: "Start a Pomodoro", act: ["pomodoro.start"], icon: "timer" }, { text: "Stop the Pomodoro", act: ["pomodoro.stop"], icon: "timer" },
  { text: "Block distractions for 25 minutes", act: ["focus.session", { minutes: 25 }], icon: "shield" },
  { text: "Block distractions for 60 minutes", act: ["focus.session", { minutes: 60 }], icon: "shield" },
  { text: "End focus session", act: ["focus.session", { minutes: 0 }], icon: "shield" },
  C("Find in page", "find", "search", "Ctrl F"), C("Print", "print", "note", "Ctrl P"), C("Developer tools", "devtools", "settings", "F12"),
  C("Zoom in", "zoom.in", "plus", "Ctrl +"), C("Zoom out", "zoom.out", "min", "Ctrl −"), C("Reset zoom", "zoom.reset", "search", "Ctrl 0"),
  C("Bookmark this page", "bookmark.add", "star", "Ctrl D"), C("History", "history", "history", "Ctrl H"), C("Bookmarks", "bookmarks", "bookmark"),
  C("Downloads", "downloads", "download", "Ctrl J"), C("Extensions", "extensions", "puzzle"), C("Themes and theme editor", "themes", "palette"),
  C("Settings", "settings", "settings"), C("Setup wizard / import", "welcome", "import"), C("Full screen", "fullscreen", "max", "F11"),
];

function subseq(q, t) {
  q = q.toLowerCase(); t = t.toLowerCase();
  let i = 0, score = 0, prev = -2;
  for (let j = 0; j < t.length && i < q.length; j++) {
    if (t[j] === q[i]) { score += j === prev + 1 ? 3 : 1; if (j === 0 || " -/".includes(t[j - 1])) score += 4; prev = j; i++; }
  }
  return i === q.length ? score : -1;
}
function hl(text, q) {
  if (!q) return text;
  const i = text.toLowerCase().indexOf(q.toLowerCase());
  if (i < 0) return text;
  return [text.slice(0, i), h("mark", text.slice(i, i + q.length)), text.slice(i + q.length)];
}
const looksLikeUrl = (q) => /^[a-z]+:\/\//i.test(q) || (/^[^\s]+\.[a-z]{2,}(\/.*)?$/i.test(q)) || /^localhost(:\d+)?/.test(q);

// ------------------------------------------------------------------ palette
function palette(arg) {
  const input = h("input", { placeholder: "Search tabs, history, bookmarks and commands, or type a URL", spellcheck: "false", autocomplete: "off" });
  const list = h("div.results");
  let items = [], sel = 0;
  root.replaceChildren(
    h("div.pal-input", h("span", { html: icon("search", 20) }), input, h("span.kbd", "Esc")),
    list,
    h("div.foot", h("span", h("span.kbd", "↑↓"), " move"), h("span", h("span.kbd", "Enter"), " open"), h("span", h("span.kbd", "Alt Enter"), " new tab")),
  );
  const draw = () => {
    list.replaceChildren();
    let group = null;
    items.forEach((it, i) => {
      if (it.group !== group) { group = it.group; list.append(h("div.grp", group)); }
      const row = h("div.res", { onclick: (e) => choose(i, e.altKey), onmousemove: () => { if (sel !== i) { sel = i; mark(); } } },
        h("span.ri", it.fav ? favicon(it.fav, 18) : h("span", { html: icon(it.icon || "search", 17) })),
        h("div.ttl", h("div", hl(it.title, input.value.trim())), it.sub ? h("div.sub", it.sub) : null),
        h("span.hint", it.hint || "Open"), it.key ? h("span.kbd", it.key) : null);
      list.append(row);
    });
    mark();
  };
  const mark = () => {
    [...list.querySelectorAll(".res")].forEach((r, i) => r.classList.toggle("sel", i === sel));
    const s = list.querySelectorAll(".res")[sel];
    if (s) s.scrollIntoView({ block: "nearest" });
  };
  const choose = (i, newTab) => {
    const it = items[i];
    if (!it) return;
    close();
    if (it.act) call(it.act[0], it.act[1] || {});
    else if (it.url) call("navigate", { input: it.url, newTab: newTab || undefined });
  };
  const update = debounce(async () => {
    const q = input.value.trim();
    const res = await call("search", { q });
    const out = [];
    if (q) {
      if (looksLikeUrl(q)) out.push({ group: "Go", title: q, sub: "Open this address", icon: "external", url: q, hint: "Go" });
      out.push({ group: looksLikeUrl(q) ? "Go" : "Search", title: q, sub: "Search the web", icon: "search", url: q, hint: "Search" });
    }
    out.push(...res.tabs.map((t) => ({ group: "Open tabs", title: t.title || prettyUrl(t.url), sub: prettyUrl(t.url), fav: t, act: ["tab.activate", { id: t.id }], hint: "Switch" })));
    const cmds = COMMANDS.map((c) => ({ c, s: q ? subseq(q, c.text) : 0 })).filter((x) => x.s >= 0).sort((a, b) => b.s - a.s).slice(0, q ? 6 : 8);
    out.push(...cmds.map(({ c }) => ({ group: "Commands", title: c.text, icon: c.icon, act: c.act, hint: "Run", key: c.key })));
    out.push(...res.parked.map((p) => ({ group: "Parked", title: p.title || prettyUrl(p.url), sub: prettyUrl(p.url), fav: p, act: ["unpark", { id: p.id }], hint: "Restore" })));
    out.push(...res.bookmarks.map((b) => ({ group: "Bookmarks", title: b.title, sub: prettyUrl(b.url), fav: { url: b.url }, url: b.url })));
    out.push(...res.history.map((b) => ({ group: "History", title: b.title || prettyUrl(b.url), sub: prettyUrl(b.url), fav: { url: b.url }, url: b.url })));
    items = out;
    sel = 0;
    draw();
  }, 40);
  input.addEventListener("input", update);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { sel = Math.min(items.length - 1, sel + 1); mark(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { sel = Math.max(0, sel - 1); mark(); e.preventDefault(); }
    else if (e.key === "Enter") { choose(sel, e.altKey); e.preventDefault(); }
    else if (e.key === "Escape") close();
  });
  const url = arg && arg.url && !arg.url.startsWith("emerald://start") ? arg.url : "";
  input.value = url;
  update();
  setTimeout(() => { input.focus(); input.select(); }, 0);
}

// ------------------------------------------------------------------ menu
function menu(arg) {
  const wrap = h("div.menu-list");
  for (const it of arg.items || []) {
    if (it.sep) { wrap.append(h("hr")); continue; }
    if (it.label) { wrap.append(h("div.grp", it.label)); continue; }
    const b = h("button", { onclick: () => { close(); if (it.act) call(it.act[0], it.act[1] || {}); } },
      it.icon ? h("span", { html: icon(it.icon, 15) }) : null, h("span", it.text), it.key ? h("span.k", it.key) : null);
    if (it.danger) b.classList.add("danger");
    wrap.append(b);
  }
  root.replaceChildren(wrap);
  wrap.tabIndex = -1;
  wrap.focus();
  wrap.addEventListener("keydown", (e) => {
    const bs = [...wrap.querySelectorAll("button")];
    let i = bs.indexOf(document.activeElement);
    if (e.key === "ArrowDown") { (bs[i + 1] || bs[0]).focus(); e.preventDefault(); }
    if (e.key === "ArrowUp") { (bs[i - 1] || bs[bs.length - 1]).focus(); e.preventDefault(); }
  });
}

// ------------------------------------------------------------------ park
function park(arg, state) {
  const tab = state.tabs.find((t) => t.id === arg.tab);
  if (!tab) { close(); return; }
  const at = (hh, addDays = 0) => { const d = new Date(); d.setDate(d.getDate() + addDays); d.setHours(hh, 0, 0, 0); return d.getTime(); };
  const now = Date.now();
  const opts = [
    { t: "No reminder", s: "It waits in Parked", v: null },
    { t: "In 1 hour", s: new Date(now + 3600e3).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }), v: now + 3600e3 },
    { t: "This evening", s: "6:00 PM", v: at(18) > now ? at(18) : at(18, 1) },
    { t: "Tomorrow morning", s: "9:00 AM", v: at(9, 1) },
    { t: "This weekend", s: "Saturday 10:00", v: (() => { const d = new Date(); d.setDate(d.getDate() + ((6 - d.getDay() + 7) % 7 || 7)); d.setHours(10, 0, 0, 0); return d.getTime(); })() },
    { t: "Next week", s: "Monday 9:00", v: (() => { const d = new Date(); d.setDate(d.getDate() + ((8 - d.getDay()) % 7 || 7)); d.setHours(9, 0, 0, 0); return d.getTime(); })() },
  ];
  let pick = 0;
  const note = h("input.input", { placeholder: "Why come back? (optional)" });
  const choices = h("div.choices", ...opts.map((o, i) => h("button.choice", { onclick: () => { pick = i; draw(); } }, o.t, h("small", o.s))));
  const draw = () => [...choices.children].forEach((c, i) => c.classList.toggle("on", i === pick));
  const go = () => { call("park", { id: tab.id, remindAt: opts[pick].v, note: note.value }); close(); };
  root.replaceChildren(h("div.panel",
    h("h2", h("span", { html: icon("park", 18) }), h("span.grow", "Park for later")),
    h("div.muted", tab.title || prettyUrl(tab.url)),
    choices, note,
    h("div.actions", h("button.btn.ghost", { onclick: close }, "Cancel"), h("button.btn.primary", { onclick: go }, "Park tab"))));
  draw();
  note.addEventListener("keydown", (e) => { if (e.key === "Enter") go(); });
  setTimeout(() => note.focus(), 0);
}

// ------------------------------------------------------------------ quick note
function note() {
  const ta = h("textarea.input", { rows: 5, placeholder: "Get it out of your head. Ctrl+Enter to save." });
  const save = () => { if (ta.value.trim()) call("notes.add", { text: ta.value }); close(); };
  ta.addEventListener("keydown", (e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) save(); });
  root.replaceChildren(h("div.panel",
    h("h2", h("span", { html: icon("note", 18) }), h("span.grow", "Quick note"), h("span.kbd", "Ctrl Enter")),
    ta,
    h("div.muted", { style: { fontSize: "12px" } }, "Notes appear on your start page."),
    h("div.actions", h("button.btn.ghost", { onclick: close }, "Cancel"), h("button.btn.primary", { onclick: save }, "Save note"))));
  setTimeout(() => ta.focus(), 0);
}

// ------------------------------------------------------------------ memory meter
let memTimer = null;
async function memory() {
  const COLORS = { browser: "#5b8def", gpu: "#e8b75a", renderer: "var(--em-accent)", extension: "#b39bf0", utility: "#8ea398", service_worker: "#3cc3c8", other: "#666" };
  const draw = async () => {
    if (mode !== "memory") return;
    const m = await call("memory");
    const total = m.total || 1;
    const types = Object.entries(m.byType || {}).sort((a, b) => b[1] - a[1]);
    const max = Math.max(1, ...m.tabs.map((t) => t.bytes));
    root.replaceChildren(h("div.panel",
      h("h2", h("span", { html: icon("chip", 18) }), h("span.grow", "Memory"), h("button.btn.small.primary", { onclick: async () => { await call("memory.free"); draw(); } }, h("span", { html: icon("moon", 14) }), "Free memory")),
      h("div.mem-total", h("span.big", fmtBytes(m.total)), h("span.muted", `${m.processes} processes · ${m.liveTabs} awake · ${m.sleeping} sleeping`)),
      h("div.stack", ...types.map(([k, v]) => h("i", { title: `${k}: ${fmtBytes(v)}`, style: { width: (v / total * 100) + "%", background: COLORS[k] || COLORS.other } }))),
      h("div.legend", ...types.map(([k, v]) => h("span", { style: { "--c": COLORS[k] || COLORS.other } }, `${k.replace("_", " ")} ${fmtBytes(v)}`))),
      h("div.mem-list", ...m.tabs.map((t) => h("div.mem-row", favicon(t, 16), h("span.t", { title: t.url }, t.title || prettyUrl(t.url)),
        h("span.bar", h("i", { style: { width: (t.bytes / max * 100) + "%" } })), h("span.mb", fmtBytes(t.bytes)),
        h("button.iconbtn", { title: "Put to sleep", html: icon("moon", 14), onclick: async () => { await call("tab.sleep", { id: t.id }); draw(); } })))),
      h("div.muted", { style: { fontSize: "12px" } }, "Sleeping tabs use no memory; they reload when you open them.")));
  };
  await draw();
  clearInterval(memTimer);
  memTimer = setInterval(draw, 2000);
}

// ------------------------------------------------------------------ dispatch
let lastState = null;
on("state", (s) => { lastState = s; });
on("open", async ({ mode: m, arg }) => {
  mode = m;
  clearInterval(memTimer);
  if (!lastState) lastState = await call("state");
  ({ palette, menu, park: (a) => park(a, lastState), note, memory })[m]?.(arg || {});
});
liveTheme();
window.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
call("ready").then((s) => { lastState = s; });
