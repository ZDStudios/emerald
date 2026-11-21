// Emerald UI runtime: IPC with the Rust shell, tiny DOM helpers, shared formatting.
// Plain ES module, no framework, no build step.

const PREFIX = "\u0001EMERALD:";
const pending = new Map();
const listeners = new Map();
let seq = 0;

window.__em = {
  resolve(id, ok, data) {
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    ok ? p.res(data) : p.rej(new Error(data));
  },
  emit(name, data) {
    for (const fn of listeners.get(name) || []) {
      try { fn(data); } catch (e) { console.error(e); }
    }
  },
};

/** Call the Rust shell. Resolves with the JSON result. */
export function call(cmd, args = {}) {
  const id = ++seq;
  return new Promise((res, rej) => {
    pending.set(id, { res, rej });
    console.info(PREFIX + JSON.stringify({ id, cmd, args }));
  });
}

/** Fire-and-forget command (keyboard/command-bar names). */
export const run = (name, args = null) => call("cmd", { name, args }).catch((e) => console.warn(name, e.message));

// Handy for DevTools and automated tests: calls still originate from this emerald:// script.
window.emerald = { call: (c, a) => call(c, a), run: (n, a) => run(n, a) };

export function on(name, fn) {
  if (!listeners.has(name)) listeners.set(name, []);
  listeners.get(name).push(fn);
  return () => listeners.set(name, listeners.get(name).filter((f) => f !== fn));
}

/** h("div.cls#id", {attrs, onclick}, ...children) */
export function h(tag, attrs, ...kids) {
  const m = tag.match(/^([a-z0-9-]+)?((?:[.#][\w-]+)*)$/i);
  const el = document.createElement(m[1] || "div");
  for (const part of m[2].match(/[.#][\w-]+/g) || []) {
    if (part[0] === ".") el.classList.add(part.slice(1)); else el.id = part.slice(1);
  }
  if (attrs && (typeof attrs !== "object" || attrs instanceof Node || Array.isArray(attrs))) { kids.unshift(attrs); attrs = null; }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v);
    else if (k === "style" && typeof v === "object") Object.assign(el.style, v);
    else if (k === "dataset") Object.assign(el.dataset, v);
    else if (k === "html") el.innerHTML = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const k of kids.flat(Infinity)) {
    if (k === null || k === undefined || k === false) continue;
    el.append(k instanceof Node ? k : document.createTextNode(String(k)));
  }
  return el;
}

export const $ = (s, r = document) => r.querySelector(s);
export const $$ = (s, r = document) => [...r.querySelectorAll(s)];

export function hostOf(url) {
  try { const u = new URL(url); return u.hostname.replace(/^www\./, ""); } catch { return ""; }
}

export function prettyUrl(url) {
  if (!url) return "";
  if (url.startsWith("emerald://start")) return "";
  try {
    const u = new URL(url);
    if (u.protocol === "emerald:") return "emerald://" + u.hostname;
    return (u.hostname.replace(/^www\./, "") + (u.pathname === "/" ? "" : u.pathname) + u.search).slice(0, 300);
  } catch { return url; }
}

export function titleOf(t) {
  if (t.title) return t.title;
  if (!t.url || t.url.startsWith("emerald://start")) return "New tab";
  return prettyUrl(t.url) || t.url;
}

/** Favicon element: image when known, otherwise a colored letter tile. */
export function favicon(t, size = 16) {
  const letter = (hostOf(t.url) || titleOf(t) || "?").replace(/^[^a-z0-9]+/i, "")[0] || "•";
  const hue = [...(hostOf(t.url) || "e")].reduce((a, c) => (a * 31 + c.charCodeAt(0)) % 360, 7);
  const tile = h("span.fav-letter", { style: { width: size + "px", height: size + "px", background: `hsl(${hue} 45% 45%)`, fontSize: Math.round(size * 0.62) + "px" } }, letter.toUpperCase());
  if (t.url && t.url.startsWith("emerald://")) {
    return h("span.fav-icon", { html: icon("leaf", size), style: { width: size + "px", height: size + "px" } });
  }
  if (!t.favicon) return tile;
  const img = h("img.fav", { src: t.favicon, width: size, height: size, alt: "", draggable: "false", loading: "lazy" });
  img.onerror = () => img.replaceWith(tile);
  return img;
}

export function fmtBytes(b) {
  if (!b || b < 0) return "0 MB";
  const mb = b / 1048576;
  return mb >= 1024 ? (mb / 1024).toFixed(1) + " GB" : mb >= 10 ? Math.round(mb) + " MB" : mb.toFixed(1) + " MB";
}

export function fmtClock(ms) {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function fmtAgo(ts) {
  const d = (Date.now() - ts) / 1000;
  if (d < 60) return "just now";
  if (d < 3600) return Math.floor(d / 60) + " min ago";
  if (d < 86400) return Math.floor(d / 3600) + " h ago";
  return new Date(ts).toLocaleDateString();
}

export function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}

/** Reload theme.css in place when the theme changes. */
export function liveTheme() {
  on("theme", (t) => {
    for (const l of $$('link[href^="theme.css"]')) l.href = "theme.css?" + Date.now();
    document.documentElement.dataset.mode = t.mode;
  });
}

// ---------------------------------------------------------------- icons
// A small hand-drawn line icon set (24×24, stroke = currentColor).
const P = {
  back: '<path d="M15 5l-7 7 7 7"/>',
  forward: '<path d="M9 5l7 7-7 7"/>',
  reload: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>',
  stop: '<path d="M6 6l12 12M18 6L6 18"/>',
  close: '<path d="M7 7l10 10M17 7L7 17"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>',
  sidebar: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M9.5 4.5v15"/>',
  focus: '<circle cx="12" cy="12" r="7.5"/><circle cx="12" cy="12" r="3"/>',
  moon: '<path d="M19 14.5A7.5 7.5 0 0 1 9.5 5a7.5 7.5 0 1 0 9.5 9.5z"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4L6 18M18 6l1.4-1.4"/>',
  pin: '<path d="M9 4h6l-1 6 3 3H7l3-3z"/><path d="M12 13v7"/>',
  folder: '<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
  chevron: '<path d="M9 6l6 6-6 6"/>',
  down: '<path d="M6 9l6 6 6-6"/>',
  dots: '<circle cx="5.5" cy="12" r="1.2"/><circle cx="12" cy="12" r="1.2"/><circle cx="18.5" cy="12" r="1.2"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2.8v2.4M12 18.8v2.4M4.2 7.5l2.1 1.2M17.7 15.3l2.1 1.2M4.2 16.5l2.1-1.2M17.7 8.7l2.1-1.2"/><circle cx="12" cy="12" r="7"/>',
  puzzle: '<path d="M9 4.5h3.5a1.5 1.5 0 0 1 3 0H19v4a1.5 1.5 0 0 1 0 3V15h-3.5a1.5 1.5 0 0 0-3 0H9v-3.5a1.5 1.5 0 0 1 0-3z"/><path d="M5 9v10h10"/>',
  shield: '<path d="M12 3l7.5 3v5.5c0 4.5-3.2 8.2-7.5 9.5-4.3-1.3-7.5-5-7.5-9.5V6z"/>',
  download: '<path d="M12 4v11M7 10.5l5 5 5-5"/><path d="M5 19.5h14"/>',
  history: '<path d="M4 12a8 8 0 1 0 2.4-5.7"/><path d="M4 4.5V9h4.5"/><path d="M12 8v4.5l3 2"/>',
  bookmark: '<path d="M7 4.5h10v15l-5-3.5-5 3.5z"/>',
  star: '<path d="M12 4l2.4 5 5.4.6-4 3.7 1.1 5.3L12 16l-4.9 2.6 1.1-5.3-4-3.7 5.4-.6z"/>',
  timer: '<circle cx="12" cy="13" r="7.5"/><path d="M12 9v4l2.5 2M9.5 2.5h5"/>',
  note: '<path d="M6 3.5h9l3.5 3.5v13.5H6z"/><path d="M9 11h6M9 14.5h6M9 8h3"/>',
  park: '<path d="M7 4.5h10v15l-5-3.5-5 3.5z"/><path d="M12 8v3l2 1"/>',
  split: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M12 4.5v15"/>',
  chip: '<rect x="6.5" y="6.5" width="11" height="11" rx="2"/><path d="M9.5 3v3.5M14.5 3v3.5M9.5 17.5V21M14.5 17.5V21M3 9.5h3.5M3 14.5h3.5M17.5 9.5H21M17.5 14.5H21"/>',
  volume: '<path d="M4.5 9.5h3l4.5-4v13l-4.5-4h-3z"/><path d="M16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11"/>',
  mute: '<path d="M4.5 9.5h3l4.5-4v13l-4.5-4h-3z"/><path d="M16.5 9.5l5 5M21.5 9.5l-5 5"/>',
  min: '<path d="M6 12h12"/>',
  max: '<rect x="6" y="6" width="12" height="12" rx="1.5"/>',
  home: '<path d="M4 11l8-6.5 8 6.5"/><path d="M6.5 9.5v10h11v-10"/>',
  grid: '<rect x="4" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5"/>',
  leaf: '<path d="M5 19c0-8 5-13.5 14-14 .5 9-5 14-12 14"/><path d="M5 19c3-4 6-6.5 9-8"/>',
  lock: '<rect x="5.5" y="10.5" width="13" height="9" rx="2"/><path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5"/>',
  warn: '<path d="M12 4l9 16H3z"/><path d="M12 10v4.5M12 17.2v.3"/>',
  reader: '<path d="M4 5.5h6.5a2 2 0 0 1 2 2V19a2 2 0 0 0-2-2H4zM20 5.5h-6.5a2 2 0 0 0-2 2V19a2 2 0 0 1 2-2H20z"/>',
  video: '<rect x="3.5" y="6" width="12.5" height="12" rx="2"/><path d="M16 10.5l4.5-2.5v8l-4.5-2.5z"/>',
  edit: '<path d="M5 19l1-4 10-10 3 3-10 10z"/><path d="M14 7l3 3"/>',
  trash: '<path d="M5 7h14M10 7V4.5h4V7M7 7l1 12.5h8L17 7"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  external: '<path d="M14 4.5h5.5V10M19.5 4.5l-8 8"/><path d="M18 14v5.5H4.5V6H10"/>',
  zzz: '<path d="M4 6h5l-5 6h5M13 11h4l-4 5h4M17 4h3l-3 3.5h3"/>',
  palette: '<path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.2 0 1.8-.9 1.4-2-.4-1.2.4-2.3 1.6-2.3H17a3.5 3.5 0 0 0 3.5-3.5c0-5-3.8-9.2-8.5-9.2z"/><circle cx="8" cy="11" r="1"/><circle cx="11" cy="7.5" r="1"/><circle cx="15.5" cy="8.5" r="1"/>',
  user: '<circle cx="12" cy="8.5" r="3.5"/><path d="M5 20c.8-3.6 3.6-5.5 7-5.5s6.2 1.9 7 5.5"/>',
  import: '<path d="M12 4v10M7.5 9.5L12 14l4.5-4.5"/><path d="M4.5 14v5.5h15V14"/>',
  bell: '<path d="M6.5 16.5V11a5.5 5.5 0 0 1 11 0v5.5l1.5 1.5H5z"/><path d="M10 20h4"/>',
  coffee: '<path d="M5 9.5h11v5a5 5 0 0 1-5 5h-1a5 5 0 0 1-5-5z"/><path d="M16 11h1.5a2.5 2.5 0 0 1 0 5H16M8.5 3.5v3M12 3.5v3"/>',
  layout: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M3.5 9h17"/>',
  drag: '<circle cx="9" cy="6.5" r="1"/><circle cx="15" cy="6.5" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="9" cy="17.5" r="1"/><circle cx="15" cy="17.5" r="1"/>',
};

export function icon(name, size = 18) {
  return `<svg class="ic ic-${name}" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${P[name] || P.dots}</svg>`;
}

export function iconEl(name, size) {
  return h("span.icw", { html: icon(name, size) });
}

/** Wire up `data-cmd` buttons on internal pages. */
export function bindCommands(root = document) {
  root.addEventListener("click", (e) => {
    const b = e.target.closest("[data-cmd]");
    if (b) run(b.dataset.cmd);
  });
}
