// The start page, drawn by the shell itself (no extra renderer process).
// Widgets live on a 12-column grid; "Customize" enables drag-to-move and resize.
import { call, run, h, $, icon, hostOf, fmtClock, prettyUrl } from "./lib.js";

let editing = false;
let clockTimer = null;
let notes = null;
let lastSig = "";

const GREETINGS = [[5, "Good morning"], [12, "Good afternoon"], [18, "Good evening"], [23, "Good night"]];
function greeting() {
  const hr = new Date().getHours();
  let g = "Hello";
  for (const [from, text] of GREETINGS) if (hr >= from) g = text;
  if (hr < 5) g = "Up late";
  return g;
}

export function renderStart(root, S, tab) {
  const sp = S.settings.start_page;
  // Re-render only when something visible changed (keeps inputs focused while typing).
  const sig = JSON.stringify([sp, S.focus, S.parked.length, editing, S.theme, tab && tab.id]);
  if (sig === lastSig && root.childElementCount) { tick(root); return; }
  lastSig = sig;
  root.classList.toggle("editing", editing);
  root.style.background = sp.background || "";
  const grid = h("div.sgrid");
  for (const w of sp.widgets) grid.append(widget(w, S, tab));
  root.replaceChildren(
    h("div.start-inner", sp.greeting ? h("div.greet", greeting()) : null, grid),
    h("div.start-tools",
      editing ? h("button.btn.small", { onclick: () => addWidgetMenu(S) }, h("span", { html: icon("plus", 14) }), "Add widget") : null,
      h("button.btn.small" + (editing ? ".primary" : ".ghost"), { onclick: () => { editing = !editing; lastSig = ""; renderStart(root, S, tab); } }, h("span", { html: icon(editing ? "check" : "edit", 14) }), editing ? "Done" : "Customize")),
  );
  if (editing) enableGridEditing(grid, S);
  tick(root);
  clearInterval(clockTimer);
  clockTimer = setInterval(() => { if (!root.hidden) tick(root); }, 1000);
  const q = $(".w-search input", root);
  if (q && !editing && document.activeElement === document.body) setTimeout(() => q.focus(), 30);
}

function tick(root) {
  const now = new Date();
  for (const el of root.querySelectorAll(".w-clock .time")) el.textContent = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  for (const el of root.querySelectorAll(".w-clock .date")) el.textContent = now.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });
}

function save(S, widgets) {
  call("settings.set", { patch: { start_page: { widgets } } });
}

function widget(w, S, tab) {
  const style = { gridColumn: `${w.x + 1} / span ${w.w}`, gridRow: `${w.y + 1} / span ${w.h}` };
  const kinds = { clock, search, links, focus, notes: notesW, parked, quote };
  const make = kinds[w.kind] || (() => h("div", "Unknown widget"));
  const el = make(w, S, tab);
  Object.assign(el.style, style);
  el.dataset.id = w.id;
  el.append(h("span.rs"), h("button.iconbtn.wdel", { title: "Remove widget", html: icon("close", 14), onclick: () => save(S, S.settings.start_page.widgets.filter((x) => x.id !== w.id)) }));
  return el;
}

function clock() {
  return h("div.widget.w-clock", h("div.time"), h("div.date"));
}

function search(w, S, tab) {
  const input = h("input", { placeholder: `Search ${S.settings.search.engine} or type a URL`, spellcheck: "false", autocomplete: "off" });
  const form = h("form", { onsubmit: (e) => { e.preventDefault(); if (input.value.trim()) call("navigate", { input: input.value, tab: tab ? tab.id : undefined }); } },
    h("span", { html: icon("search", 20) }), input, h("span.kbd", "Ctrl K"));
  return h("div.widget.w-search", form);
}

function links(w, S) {
  const items = (w.config && w.config.links) || [];
  const setLinks = (next) => save(S, S.settings.start_page.widgets.map((x) => (x.id === w.id ? { ...x, config: { ...(x.config || {}), links: next } } : x)));
  const grid = h("div.links", ...items.map((l, i) => {
    const host = hostOf(l.url);
    const img = h("img", { src: `https://${host}/favicon.ico`, width: 22, height: 22, alt: "" });
    img.onerror = () => img.replaceWith(h("span", { style: { fontWeight: 700, fontSize: "18px", color: "var(--em-accent)" } }, (l.title || host || "?")[0].toUpperCase()));
    return h("a.link", { href: "#", title: l.url, onclick: (e) => { e.preventDefault(); if (!editing) call("navigate", { input: l.url }); } },
      h("span.tile", img), h("span.nm", l.title || host),
      h("button.iconbtn.del", { title: "Remove", html: icon("close", 12), onclick: (e) => { e.preventDefault(); e.stopPropagation(); setLinks(items.filter((_, j) => j !== i)); } }));
  }));
  const add = h("button.link", { title: "Add a link", onclick: () => {
    const url = prompt("Link address (e.g. github.com)");
    if (!url) return;
    const full = /^[a-z]+:\/\//i.test(url) ? url : "https://" + url;
    setLinks([...items, { title: hostOf(full).split(".")[0].replace(/^./, (c) => c.toUpperCase()), url: full }]);
  } }, h("span.tile", { html: icon("plus", 18) }), h("span.nm.muted", "Add"));
  if (editing || items.length < 12) grid.append(add);
  return h("div.widget.boxed", h("h3", "Quick links"), grid);
}

function focus(w, S) {
  const f = S.focus || {};
  const p = f.pomodoro;
  const pc = S.settings.adhd.pomodoro;
  const body = [];
  if (p) {
    body.push(h("div.phase", p.phase === "work" ? `Focus block ${p.cycle}` : p.phase === "long" ? "Long break" : "Short break"),
      h("div.timer", fmtClock(p.left)),
      h("div.btns",
        h("button.btn.small", { onclick: () => call("pomodoro.pause") }, p.paused ? "Resume" : "Pause"),
        h("button.btn.small.ghost", { onclick: () => call("pomodoro.skip") }, "Skip"),
        h("button.btn.small.ghost", { onclick: () => call("pomodoro.stop") }, "Stop")));
  } else {
    body.push(h("div.phase", f.blocking ? `${f.blocking}: distractions blocked` : "Ready when you are"),
      h("div.timer", `${pc.work_min}:00`),
      h("div.btns",
        h("button.btn.small.primary", { onclick: () => call("pomodoro.start") }, h("span", { html: icon("timer", 14) }), "Start focus"),
        h("button.btn.small.ghost", { title: "Block distracting sites for 45 minutes without a timer", onclick: () => call("focus.session", { minutes: f.sessionUntil ? 0 : 45 }) }, f.sessionUntil ? "End session" : "Block 45 min")));
  }
  const el = h("div.widget.boxed.w-focus", h("h3", h("span", { html: icon("focus", 14) }), "Focus"), ...body);
  el.querySelector(".timer").dataset.live = "1";
  return el;
}

function notesW(w, S) {
  const list = h("div.w-list");
  const input = h("input.input", { placeholder: "Capture a thought…", spellcheck: "true" });
  const draw = (items) => {
    notes = items;
    list.replaceChildren(...(items.length ? items.slice(0, 30).map((n) => {
      const li = h("div.li", h("input", { type: "checkbox", checked: n.done, onchange: (e) => call("notes.update", { id: n.id, done: e.target.checked }).then(draw) }), h("span.t", { title: n.text }, n.text),
        h("button.iconbtn", { title: "Delete", html: icon("trash", 13), onclick: () => call("notes.delete", { id: n.id }).then(draw) }));
      if (n.done) li.classList.add("done");
      return li;
    }) : [h("div.muted", { style: { padding: "6px 4px", fontSize: "12.5px" } }, "Nothing yet. Ctrl+Shift+N captures from anywhere.")]));
  };
  if (notes) draw(notes); else call("notes.list").then(draw);
  const form = h("form", { onsubmit: (e) => { e.preventDefault(); if (input.value.trim()) call("notes.add", { text: input.value }).then((n) => { input.value = ""; draw(n); }); } }, input);
  return h("div.widget.boxed.w-notes", h("h3", h("span", { html: icon("note", 14) }), "Quick notes"), form, list);
}

function parked(w, S) {
  const list = h("div.w-list", ...(S.parked.length ? S.parked.slice(0, 20).map((p) => h("div.li", { title: p.url, style: { cursor: "pointer" }, onclick: () => call("unpark", { id: p.id }) },
    h("span", { html: icon("park", 14) }), h("span.t", p.title || prettyUrl(p.url)),
    h("button.iconbtn", { title: "Forget", html: icon("close", 13), onclick: (e) => { e.stopPropagation(); call("parked.delete", { id: p.id }); } })))
    : [h("div.muted", { style: { padding: "6px 4px", fontSize: "12.5px" } }, "Park tabs you want to come back to (Ctrl+Shift+P). They'll wait here, with an optional reminder.")]));
  return h("div.widget.boxed", h("h3", h("span", { html: icon("park", 14) }), "Parked for later"), list);
}

const QUOTES = [
  "One tab at a time.", "Small steps still move you forward.", "Done is better than perfect.",
  "You don't have to do it all today.", "Breathe. Then pick the next tiny step.", "Focus is a muscle. Rest counts too.",
];
function quote() {
  const q = QUOTES[new Date().getDate() % QUOTES.length];
  return h("div.widget.w-clock", h("div.date", { style: { fontSize: "16px", fontStyle: "italic" } }, q));
}

const CATALOG = [
  ["clock", "Clock", 12, 2], ["search", "Search", 8, 1], ["links", "Quick links", 8, 2],
  ["focus", "Focus timer", 4, 3], ["notes", "Quick notes", 4, 3], ["parked", "Parked tabs", 4, 3], ["quote", "Gentle reminder", 12, 1],
];
function addWidgetMenu(S) {
  const ws = S.settings.start_page.widgets;
  const maxY = ws.reduce((m, w) => Math.max(m, w.y + w.h), 0);
  const kind = prompt("Add which widget? " + CATALOG.map((c) => c[0]).join(", "), "parked");
  const c = CATALOG.find((x) => x[0] === kind);
  if (!c) return;
  const id = c[0] + "-" + Date.now().toString(36);
  save(S, [...ws, { id, kind: c[0], x: c[2] === 12 ? 0 : 2, y: maxY, w: c[2], h: c[3], config: c[0] === "links" ? { links: [] } : null }]);
}

/** Drag to move / drag the corner to resize, snapping to the 12-column grid. */
function enableGridEditing(grid, S) {
  grid.addEventListener("pointerdown", (e) => {
    const el = e.target.closest(".widget");
    if (!el || e.target.closest(".wdel")) return;
    e.preventDefault();
    const resize = e.target.classList.contains("rs");
    const ws = S.settings.start_page.widgets.map((x) => ({ ...x }));
    const w = ws.find((x) => x.id === el.dataset.id);
    const gr = grid.getBoundingClientRect();
    const colW = (gr.width + 14) / 12, rowH = 64 + 14;
    const start = { x: e.clientX, y: e.clientY, wx: w.x, wy: w.y, ww: w.w, wh: w.h };
    el.setPointerCapture(e.pointerId);
    const move = (ev) => {
      const dx = Math.round((ev.clientX - start.x) / colW), dy = Math.round((ev.clientY - start.y) / rowH);
      if (resize) { w.w = Math.max(2, Math.min(12 - w.x, start.ww + dx)); w.h = Math.max(1, Math.min(8, start.wh + dy)); }
      else { w.x = Math.max(0, Math.min(12 - w.w, start.wx + dx)); w.y = Math.max(0, start.wy + dy); }
      el.style.gridColumn = `${w.x + 1} / span ${w.w}`;
      el.style.gridRow = `${w.y + 1} / span ${w.h}`;
    };
    const up = () => { el.removeEventListener("pointermove", move); el.removeEventListener("pointerup", up); save(S, ws); };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  });
}

export { run };
