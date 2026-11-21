// Emerald shell UI: sidebar, top bar, notices; reports the content rectangle to Rust,
// which places the native tab windows over it.
import { call, run, on, h, $, $$, icon, favicon, titleOf, prettyUrl, hostOf, fmtClock, liveTheme } from "./lib.js";
import { renderStart } from "./start.js";

let S = null;            // last state from Rust
let dragging = null;     // {id, kind}
let renderQueued = false;
const body = document.body;

for (const el of $$("[data-icon]")) el.innerHTML = icon(el.dataset.icon, el.classList.contains("iconbtn") ? 18 : 16);

// ------------------------------------------------------------------ layout reporting
const content = $("#content");
let lastRect = "";
function reportLayout() {
  const r = content.getBoundingClientRect();
  const rect = { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height), dpr: window.devicePixelRatio || 1 };
  const key = JSON.stringify(rect);
  if (key === lastRect) return;
  lastRect = key;
  call("layout", rect);
}
new ResizeObserver(reportLayout).observe(content);
window.addEventListener("resize", reportLayout);
const relayout = () => requestAnimationFrame(reportLayout);

// ------------------------------------------------------------------ state → DOM
const tabById = (id) => S && S.tabs.find((t) => t.id === id);
const folderById = (id) => S && S.folders.find((f) => f.id === id);

function scheduleRender() {
  if (renderQueued) return;
  renderQueued = true;
  requestAnimationFrame(() => { renderQueued = false; render(); });
}

function render() {
  if (!S) return;
  const st = S.settings;
  body.classList.toggle("layout-horizontal", st.layout.tabs === "horizontal");
  body.classList.toggle("layout-vertical", st.layout.tabs !== "horizontal");
  body.classList.toggle("sb-right", !!st.layout.sidebar_right);
  body.classList.toggle("sb-auto", !!st.layout.sidebar_autohide);
  body.classList.toggle("tb-auto", !!st.layout.topbar_autohide);
  body.classList.toggle("no-winctl", !st.layout.show_window_controls);
  body.classList.toggle("focus", !!S.focusMode);
  document.documentElement.style.setProperty("--sb-width", Math.max(180, Math.min(420, st.layout.sidebar_width || 240)) + "px");
  $("#btn-focus").classList.toggle("on", !!S.focusMode);
  if (!dragging) { renderSpaces(); renderPinned(); renderTabs(); renderStrip(); }
  renderTop();
  renderFoot();
  renderContent();
  body.classList.remove("booting");
  relayout();
}

function renderSpaces() {
  const el = $("#spaces");
  el.replaceChildren(
    ...S.workspaces.map((w) => {
      const b = h("button.space", { role: "tab", title: `${w.name} (right-click to edit)`, style: { "--c": w.color }, "aria-selected": w.id === S.activeWorkspace ? "true" : "false" },
        w.icon ? h("span", w.icon) : h("span.dot"), S.workspaces.length <= 4 || w.id === S.activeWorkspace ? h("span", w.name) : null);
      b.classList.toggle("on", w.id === S.activeWorkspace);
      b.onclick = () => call("ws.switch", { id: w.id });
      b.oncontextmenu = (e) => { e.preventDefault(); spaceMenu(e, w); };
      b.ondragover = (e) => { if (dragging) { e.preventDefault(); b.classList.add("drop"); } };
      b.ondragleave = () => b.classList.remove("drop");
      b.ondrop = (e) => { e.preventDefault(); b.classList.remove("drop"); if (dragging) call("tab.move", { id: dragging.id, target: { workspace: w.id } }); };
      return b;
    }),
    h("button.space.add", { title: "New space", html: icon("plus", 14), onclick: (e) => newSpaceMenu(e) }),
  );
}

function tabClasses(t) {
  const c = [];
  if (t.id === S.activeTab) c.push("on");
  if (S.split && (S.split.left === t.id || S.split.right === t.id) && t.id !== S.activeTab) c.push("split-on");
  if (!t.live && !isVirtual(t)) c.push("sleeping");
  if (t.crashed) c.push("crashed");
  return c;
}
const isVirtual = (t) => !t.url || t.url.startsWith("emerald://start");

function renderPinned() {
  $("#pinned").replaceChildren(...S.pinned.map((id) => {
    const t = tabById(id);
    if (!t) return "";
    const el = h("div.pin", { title: titleOf(t), draggable: "true" }, favicon(t, 18));
    tabClasses(t).forEach((c) => el.classList.add(c));
    el.onclick = () => call("tab.activate", { id });
    el.onauxclick = (e) => { if (e.button === 1) call("tab.close", { id }); };
    el.oncontextmenu = (e) => { e.preventDefault(); tabMenu(e, t); };
    dnd(el, { id, kind: "tab", pinned: true });
    return el;
  }));
  const p = $("#pinned");
  p.ondragover = (e) => { if (dragging && dragging.kind === "tab") { e.preventDefault(); p.classList.add("drop-into"); } };
  p.ondragleave = () => p.classList.remove("drop-into");
  p.ondrop = (e) => { if (e.target.closest(".pin")) return; e.preventDefault(); p.classList.remove("drop-into"); call("tab.move", { id: dragging.id, target: { pinned: true } }); };
}

function tabRow(t, depth) {
  const snd = h("button.iconbtn.snd", { title: t.muted ? "Unmute" : "Mute", html: icon(t.muted ? "mute" : "volume", 14), onclick: (e) => { e.stopPropagation(); call("tab.mute", { id: t.id }); } });
  if (t.audible || t.muted) snd.classList.add("audible");
  const el = h("div.tab", { role: "listitem", title: `${titleOf(t)}\n${prettyUrl(t.url)}${!t.live && !isVirtual(t) ? "\n(sleeping: click to wake)" : ""}`, draggable: "true", dataset: { id: t.id } },
    t.loading ? h("span.spin") : null,
    favicon(t, 16),
    h("span.t", titleOf(t)),
    !t.live && !isVirtual(t) ? h("span.zz", { html: icon("moon", 13), title: "Sleeping" }) : null,
    snd,
    h("button.iconbtn.x", { title: "Close (Ctrl+W)", html: icon("close", 14), onclick: (e) => { e.stopPropagation(); call("tab.close", { id: t.id }); } }),
  );
  if (depth) el.classList.add("depth1");
  tabClasses(t).forEach((c) => el.classList.add(c));
  el.onclick = (e) => {
    if (e.ctrlKey && S.activeTab && S.activeTab !== t.id) call("split", { left: S.activeTab, right: t.id });
    else call("tab.activate", { id: t.id });
  };
  el.onauxclick = (e) => { if (e.button === 1) call("tab.close", { id: t.id }); };
  el.oncontextmenu = (e) => { e.preventDefault(); tabMenu(e, t); };
  dnd(el, { id: t.id, kind: "tab", folder: t.folder });
  return el;
}

function renderTabs() {
  const wrap = $("#tabs");
  const rows = [];
  let collapsed = null;
  for (const r of S.rows) {
    if (r.kind === "folder") {
      const f = folderById(r.id);
      if (!f) continue;
      collapsed = f.collapsed ? f.id : null;
      const count = S.tabs.filter((t) => t.folder === f.id).length;
      const el = h("div.folder", { dataset: { id: f.id }, draggable: "true", style: { "--c": f.color } },
        h("span.icw.chev", { html: icon("chevron", 12) }),
        h("span.sw"),
        h("span.t", f.name),
        f.collapsed ? h("span.n.muted", String(count)) : null);
      if (!f.collapsed) el.classList.add("open");
      if (f.group) el.classList.add("group");
      el.onclick = () => call("folder.update", { id: f.id, collapsed: !f.collapsed });
      el.oncontextmenu = (e) => { e.preventDefault(); folderMenu(e, f); };
      dnd(el, { id: f.id, kind: "folder" });
      rows.push(el);
    } else {
      const t = tabById(r.id);
      if (!t) continue;
      if (r.depth && collapsed !== null && t.folder === collapsed) {
        if (t.id !== S.activeTab) continue;
      }
      rows.push(tabRow(t, r.depth));
    }
  }
  if (!rows.length && !S.pinned.length) rows.push(h("div.hint", "No tabs in this space. Press Ctrl+T or Ctrl+K."));
  wrap.replaceChildren(...rows);
  wrap.ondragover = (e) => { if (dragging && e.target === wrap) e.preventDefault(); };
  wrap.ondrop = (e) => { if (e.target === wrap && dragging) { e.preventDefault(); call("tab.move", { id: dragging.id, target: {} }); } };
  const on = $(".tab.on", wrap);
  if (on) on.scrollIntoView({ block: "nearest" });
}

function renderStrip() {
  const strip = $("#tabstrip");
  if (S.settings.layout.tabs !== "horizontal") { strip.replaceChildren(); return; }
  const ids = [...S.pinned, ...S.rows.filter((r) => r.kind === "tab").map((r) => r.id)];
  strip.replaceChildren(...ids.map((id) => {
    const t = tabById(id);
    const el = h("div.htab", { title: titleOf(t), draggable: "true" }, favicon(t, 16), h("span.t", titleOf(t)),
      h("button.iconbtn.x", { html: icon("close", 13), onclick: (e) => { e.stopPropagation(); call("tab.close", { id }); } }));
    if (t.pinned) el.classList.add("pinned");
    tabClasses(t).forEach((c) => el.classList.add(c));
    el.onclick = () => call("tab.activate", { id });
    el.onauxclick = (e) => { if (e.button === 1) call("tab.close", { id }); };
    el.oncontextmenu = (e) => { e.preventDefault(); tabMenu(e, t); };
    dnd(el, { id, kind: "tab", horizontal: true });
    return el;
  }), h("button.htab.newbtn.iconbtn", { title: "New tab", html: icon("plus", 16), onclick: () => run("tab.new") }));
}

function renderTop() {
  const t = tabById(S.activeTab);
  $("#btn-back").disabled = !(t && t.canBack);
  $("#btn-forward").disabled = !(t && t.canForward);
  $("#btn-reload").innerHTML = icon(t && t.loading ? "stop" : "reload", 18);
  $("#btn-reload").title = t && t.loading ? "Stop" : "Reload (Ctrl+R)";
  const text = $("#addr-text");
  const ic = $("#addr-icon");
  ic.className = "addr-icon";
  if (!t || isVirtual(t)) {
    text.replaceChildren(h("span.ph", "Search or enter address"));
    ic.innerHTML = icon("search", 15);
  } else {
    let u;
    try { u = new URL(t.url); } catch { u = null; }
    if (u && (u.protocol === "https:" || u.protocol === "http:")) {
      const host = u.hostname.replace(/^www\./, "");
      const rest = (u.pathname === "/" ? "" : u.pathname) + u.search;
      text.replaceChildren(h("span.host", host), h("span.rest", rest));
      ic.innerHTML = icon(u.protocol === "https:" ? "lock" : "warn", 14);
      ic.classList.add(u.protocol === "https:" ? "secure" : "insecure");
      ic.title = u.protocol === "https:" ? "Connection is secure" : "Not secure: this page is not using HTTPS";
    } else {
      text.replaceChildren(h("span.host", prettyUrl(t.url)));
      ic.innerHTML = icon("leaf", 14);
    }
  }
  const p = $("#progress");
  if (t && t.loading) { p.classList.add("on"); p.style.width = Math.max(8, (t.progress || 0) * 100) + "%"; }
  else { p.style.width = "100%"; p.classList.remove("on"); setTimeout(() => { if (!p.classList.contains("on")) p.style.width = "0"; }, 400); }
  // Extension pins
  $("#ext-pins").replaceChildren(...(S.extensions || []).map((x) =>
    h("button.iconbtn", { title: x.name, onclick: (e) => { const r = e.currentTarget.getBoundingClientRect(); call("ext.popup", { id: x.id, x: r.right, y: r.bottom }); } },
      x.icon ? h("img", { src: x.icon, alt: "" }) : h("span", { html: icon("puzzle", 16) }))));
}

function renderFoot() {
  const f = S.focus || {};
  const items = [];
  if (f.pomodoro) {
    const p = f.pomodoro;
    const chip = h("button.chip", { title: "Pomodoro: click to pause/resume", onclick: () => call("pomodoro.pause") }, h("span", { html: icon(p.phase === "work" ? "timer" : "coffee", 14) }), h("span.left", fmtClock(p.left)));
    chip.classList.add(p.phase === "work" ? "work" : "break");
    if (p.paused) chip.style.opacity = ".6";
    items.push(chip);
  } else if (f.blocking) {
    items.push(h("span.chip.work", { title: "Distracting sites are blocked" }, h("span", { html: icon("shield", 14) }), f.blocking));
  }
  items.push(h("span.grow"));
  const btn = (ic, title, fn, badge) => h("button.iconbtn", { title, html: icon(ic, 17), onclick: fn }, badge ? h("span.badge", String(badge)) : null);
  items.push(
    btn("park", "Parked tabs", (e) => parkedMenu(e), S.parked.length || null),
    btn("note", "Quick note (Ctrl+Shift+N)", () => run("note.quick")),
    btn("chip", "Memory (Ctrl+Shift+M)", () => run("memory")),
    btn("download", "Downloads (Ctrl+J)", () => run("downloads"), S.downloads || null),
    btn("settings", "Settings", () => run("settings")),
  );
  $("#sb-foot").replaceChildren(...items);
}

function renderContent() {
  const t = tabById(S.activeTab);
  const start = $("#start"), ph = $("#placeholder");
  const showStart = !t || (isVirtual(t) && !(S.split && !S.focusMode));
  start.hidden = !showStart;
  if (showStart) renderStart(start, S, t);
  const waking = t && !isVirtual(t) && !t.live;
  ph.hidden = !waking && !(t && t.crashed);
  if (t && t.crashed) ph.replaceChildren(h("div.big", "This page stopped working"), h("button.btn", { onclick: () => run("nav.reload") }, "Reload"));
  else if (waking) ph.replaceChildren(h("span", { html: icon("zzz", 28) }), h("div.big", "Waking up " + (hostOf(t.url) || "tab") + "…"));
}

// ------------------------------------------------------------------ drag and drop
function dnd(el, info) {
  el.addEventListener("dragstart", (e) => {
    dragging = info;
    el.classList.add("dragging");
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", tabById(info.id)?.url || "");
  });
  el.addEventListener("dragend", () => { dragging = null; el.classList.remove("dragging"); clearDrop(); scheduleRender(); });
  el.addEventListener("dragover", (e) => {
    if (!dragging || dragging.id === info.id) return;
    if (dragging.kind === "folder" && (info.folder || info.pinned)) return;
    e.preventDefault();
    e.stopPropagation();
    clearDrop();
    const r = el.getBoundingClientRect();
    if (info.horizontal || info.pinned) {
      el.classList.add(e.clientX < r.left + r.width / 2 ? "drop-before" : "drop-after");
      return;
    }
    const y = (e.clientY - r.top) / r.height;
    if (info.kind === "folder" && dragging.kind === "tab" && y > 0.3 && y < 0.7) el.classList.add("drop-into");
    else el.classList.add(y < 0.5 ? "drop-before" : "drop-after");
  });
  el.addEventListener("drop", (e) => {
    if (!dragging || dragging.id === info.id) return;
    e.preventDefault();
    e.stopPropagation();
    const into = el.classList.contains("drop-into");
    const after = el.classList.contains("drop-after");
    clearDrop();
    const target = { pinned: !!info.pinned };
    if (into) target.folder = info.id;
    else {
      if (info.kind === "tab" && info.folder && dragging.kind === "tab") target.folder = info.folder;
      target.before = after ? nextSiblingId(el) : info.id;
    }
    call("tab.move", { id: dragging.id, target });
  });
}
function nextSiblingId(el) {
  let n = el.nextElementSibling;
  while (n && n.dataset && n.dataset.id && n.classList.contains("depth1") !== el.classList.contains("depth1") && !n.classList.contains("folder")) n = n.nextElementSibling;
  return n && n.dataset && n.dataset.id ? Number(n.dataset.id) : null;
}
function clearDrop() { for (const x of $$(".drop-before, .drop-after, .drop-into")) x.classList.remove("drop-before", "drop-after", "drop-into"); }

// ------------------------------------------------------------------ menus
// Items are data: {text, icon, key, danger, act: [api, args]} | "-" | {label} | {node}.
// Menus that fit inside the sidebar are drawn here; anything else is drawn by the overlay,
// because native page windows always paint above this view.
const menu = $("#menu");
const cmd = (name, args) => ["cmd", { name, args }];
function exec(it) { if (it.act) call(it.act[0], it.act[1] || {}); }
function openMenu(e, items) {
  items = items.filter(Boolean);
  const sb = $("#sidebar").getBoundingClientRect();
  const inSidebar = !body.classList.contains("layout-horizontal") && !body.classList.contains("sb-hidden") && e.clientX >= sb.left && e.clientX <= sb.right;
  if (!inSidebar) {
    const data = items.filter((it) => !it.node).map((it) => (it === "-" ? { sep: true } : it));
    call("overlay.open", { mode: "menu", arg: { items: data, x: e.clientX, y: e.clientY } });
    return;
  }
  menu.replaceChildren(...items.map((it) => {
    if (it === "-") return h("hr");
    if (it.node) return it.node;
    if (it.label) return h("div.lbl", it.label);
    const b = h("button", { onclick: () => { closeMenu(); exec(it); } }, it.icon ? h("span", { html: icon(it.icon, 15) }) : null, h("span", it.text), it.key ? h("span.k", it.key) : null);
    if (it.danger) b.classList.add("danger");
    return b;
  }));
  menu.hidden = false;
  const mw = menu.offsetWidth, mh = menu.offsetHeight;
  menu.style.left = Math.max(sb.left + 4, Math.min(e.clientX, sb.right - mw - 4)) + "px";
  menu.style.top = Math.max(4, Math.min(e.clientY, window.innerHeight - mh - 4)) + "px";
}
function closeMenu() { menu.hidden = true; }
document.addEventListener("mousedown", (e) => { if (!menu.hidden && !menu.contains(e.target)) closeMenu(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") { closeMenu(); closeFind(); } });

function tabMenu(e, t) {
  const others = S.tabs.filter((x) => x.workspace === S.activeWorkspace && x.id !== t.id);
  openMenu(e, [
    { text: t.pinned ? "Unpin" : "Pin", icon: "pin", act: ["tab.pin", { id: t.id, pinned: !t.pinned }] },
    { text: "Duplicate", icon: "plus", act: ["tab.duplicate", { id: t.id }] },
    S.activeTab && S.activeTab !== t.id ? { text: "Open in split view", icon: "split", act: ["split", { left: S.activeTab, right: t.id }] } : null,
    S.split ? { text: "Close split view", icon: "split", act: ["unsplit"] } : null,
    { text: t.muted ? "Unmute" : "Mute", icon: t.muted ? "volume" : "mute", act: ["tab.mute", { id: t.id }] },
    t.live && t.id !== S.activeTab ? { text: "Put to sleep", icon: "moon", act: ["tab.sleep", { id: t.id }] } : null,
    { text: "Park for later…", icon: "park", act: ["overlay.open", { mode: "park", arg: { tab: t.id } }] },
    "-",
    { text: "New folder with this tab", icon: "folder", act: ["folder.new", { name: "New folder", tabs: [t.id] }] },
    { text: "New group with this tab", icon: "grid", act: ["folder.new", { name: "Group", group: true, color: "#e8b75a", tabs: [t.id] }] },
    t.folder ? { text: "Move out of folder", icon: "folder", act: ["tab.move", { id: t.id, target: {} }] } : null,
    ...S.workspaces.filter((w) => w.id !== t.workspace).map((w) => ({ text: `Move to ${w.icon || ""} ${w.name}`, icon: "grid", act: ["tab.move", { id: t.id, target: { workspace: w.id } }] })),
    "-",
    others.length ? { text: "Close other tabs", act: ["tabs.closeOthers", { id: t.id }] } : null,
    { text: "Close", icon: "close", key: "Ctrl W", danger: true, act: ["tab.close", { id: t.id }] },
  ]);
}

function folderMenu(e, f) {
  const name = h("input.input", { value: f.name, placeholder: "Name" });
  name.onkeydown = (ev) => { if (ev.key === "Enter") { call("folder.update", { id: f.id, name: name.value }); closeMenu(); } ev.stopPropagation(); };
  const colors = ["#34c084", "#5b8def", "#e8b75a", "#ef6f6c", "#b39bf0", "#3cc3c8", "#8ea398"];
  openMenu(e, [
    { node: name },
    { node: h("div", { style: { display: "flex", gap: "6px", padding: "2px 8px 6px" } }, ...colors.map((c) => h("button", { title: c, style: { width: "20px", height: "20px", padding: 0, borderRadius: "50%", background: c, border: c === f.color ? "2px solid var(--em-text)" : "0" }, onclick: () => { call("folder.update", { id: f.id, color: c }); closeMenu(); } }))) },
    "-",
    { text: "Dissolve (keep tabs)", icon: "folder", act: ["folder.delete", { id: f.id, closeTabs: false }] },
    { text: "Delete with its tabs", icon: "trash", danger: true, act: ["folder.delete", { id: f.id, closeTabs: true }] },
  ]);
  setTimeout(() => name.focus(), 0);
}

const EMOJI = ["🌿", "💼", "🏠", "📚", "🎨", "🧪", "🎮", "✈️", "💡", "🎵"];
function spaceMenu(e, w) {
  const name = h("input.input", { value: w.name });
  name.onkeydown = (ev) => { if (ev.key === "Enter") { call("ws.update", { id: w.id, name: name.value }); closeMenu(); } ev.stopPropagation(); };
  openMenu(e, [
    { node: name },
    { node: h("div", { style: { display: "flex", flexWrap: "wrap", gap: "2px", padding: "0 6px 6px" } }, ...EMOJI.map((x) => h("button", { style: { width: "28px", height: "28px", padding: 0, justifyContent: "center" }, onclick: () => { call("ws.update", { id: w.id, icon: x }); closeMenu(); } }, x))) },
    "-",
    S.workspaces.length > 1 ? { text: "Delete space and its tabs", icon: "trash", danger: true, act: ["ws.delete", { id: w.id }] } : null,
  ]);
  setTimeout(() => name.select(), 0);
}

function newSpaceMenu(e) {
  const name = h("input.input", { placeholder: "Space name, e.g. Work" });
  const go = () => { if (name.value.trim()) call("ws.new", { name: name.value.trim(), icon: EMOJI[S.workspaces.length % EMOJI.length] }); closeMenu(); };
  name.onkeydown = (ev) => { if (ev.key === "Enter") go(); ev.stopPropagation(); };
  openMenu(e, [{ label: "New space" }, { node: name }, { node: h("button", { onclick: go }, h("span", { html: icon("plus", 15) }), h("span", "Create")) }]);
  setTimeout(() => name.focus(), 0);
}

function parkedMenu(e) {
  const items = S.parked.slice(0, 12).map((p) => ({ text: p.title || prettyUrl(p.url), icon: "park", act: ["unpark", { id: p.id }] }));
  openMenu(e, [{ label: S.parked.length ? "Parked for later" : "Nothing parked yet. Press Ctrl+Shift+P on a tab." }, ...items]);
}

function mainMenu(e) {
  const r = e.currentTarget.getBoundingClientRect();
  openMenu({ clientX: r.right, clientY: r.bottom + 4 }, [
    { text: "New tab", icon: "plus", key: "Ctrl T", act: cmd("tab.new") },
    { text: "Command bar", icon: "search", key: "Ctrl K", act: cmd("palette") },
    { text: "Split view", icon: "split", key: "Ctrl Shift E", act: cmd("split.toggle") },
    "-",
    { text: "Start a Pomodoro", icon: "timer", act: ["pomodoro.start"] },
    { text: "Distraction-free video", icon: "video", act: cmd("video.focus") },
    { text: "Quick note", icon: "note", key: "Ctrl Shift N", act: cmd("note.quick") },
    { text: "Free memory", icon: "chip", act: cmd("memory.free") },
    "-",
    { text: "History", icon: "history", key: "Ctrl H", act: cmd("history") },
    { text: "Bookmarks", icon: "bookmark", act: cmd("bookmarks") },
    { text: "Downloads", icon: "download", key: "Ctrl J", act: cmd("downloads") },
    { text: "Extensions", icon: "puzzle", act: cmd("extensions") },
    { text: "Themes", icon: "palette", act: cmd("themes") },
    "-",
    { text: "Find in page", icon: "search", key: "Ctrl F", act: cmd("find") },
    { text: "Print…", icon: "note", key: "Ctrl P", act: cmd("print") },
    { text: "Developer tools", icon: "settings", key: "F12", act: cmd("devtools") },
    { text: "Settings", icon: "settings", act: cmd("settings") },
  ]);
}

// ------------------------------------------------------------------ top bar actions
$("#btn-back").onclick = () => run("nav.back");
$("#btn-forward").onclick = () => run("nav.forward");
$("#btn-reload").onclick = () => run(tabById(S?.activeTab)?.loading ? "nav.stop" : "nav.reload");
$("#btn-sidebar").onclick = () => toggleSidebar();
$("#btn-reader").onclick = () => run("reader.toggle");
$("#btn-focus").onclick = () => call("focus.mode", {});
$("#btn-menu").onclick = mainMenu;
$("#address").onclick = () => run("palette", { url: tabById(S?.activeTab)?.url || "" });
$("#newtab").onclick = () => run("tab.new");
$("#win-min").onclick = () => call("win.minimize");
$("#win-max").onclick = () => call("win.maximize");
$("#win-close").onclick = () => call("win.close");
$("#topbar").addEventListener("dblclick", (e) => { if (e.target === e.currentTarget || e.target.classList.contains("navrow")) call("win.maximize"); });

function toggleSidebar() {
  if (S.settings.layout.tabs === "horizontal") return;
  body.classList.toggle("sb-hidden");
  relayout();
}
on("toggleSidebar", toggleSidebar);

// Auto-hide: hovering the edge reveals the sidebar / top bar (pushing the page, never covering it).
let peekTimer;
$("#sb-edge").addEventListener("mouseenter", () => {
  if (body.classList.contains("sb-hidden")) return;
  body.classList.add("sb-peek"); relayout();
});
$("#sidebar").addEventListener("mouseleave", () => {
  if (!body.classList.contains("sb-auto")) return;
  clearTimeout(peekTimer);
  peekTimer = setTimeout(() => { if (!menu.hidden) return; body.classList.remove("sb-peek"); relayout(); }, 500);
});
$("#sb-edge").addEventListener("click", () => { body.classList.remove("sb-hidden"); relayout(); });
$("#top-edge").addEventListener("mouseenter", () => { body.classList.add("tb-peek"); $("#topbar").classList.add("peek"); relayout(); });
$("#topbar").addEventListener("mouseleave", () => {
  if (!body.classList.contains("tb-auto") && !body.classList.contains("focus")) return;
  setTimeout(() => { body.classList.remove("tb-peek"); $("#topbar").classList.remove("peek"); relayout(); }, 600);
});

// ------------------------------------------------------------------ find in page
const findbar = $("#findbar"), findInput = $("#find-input");
function openFind() { findbar.hidden = false; findInput.focus(); findInput.select(); relayout(); }
function closeFind() { if (findbar.hidden) return; findbar.hidden = true; $("#find-count").textContent = ""; call("find.stop"); relayout(); }
on("find", openFind);
findInput.addEventListener("input", () => call("find", { text: findInput.value }));
findInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") call("find", { text: findInput.value, forward: !e.shiftKey, next: true });
  if (e.key === "Escape") closeFind();
});
$("#find-next").onclick = () => call("find", { text: findInput.value, forward: true, next: true });
$("#find-prev").onclick = () => call("find", { text: findInput.value, forward: false, next: true });
$("#find-close").onclick = closeFind;
on("findResult", (r) => { $("#find-count").textContent = r.count ? `${r.active} of ${r.count}` : findInput.value ? "No matches" : ""; });

// ------------------------------------------------------------------ notices & toasts
function notice(kind, ic, text, actions = [], sticky = true) {
  const el = h("div.notice", { role: "status" }, h("span", { html: icon(ic, 16) }), h("span.grow", text),
    ...actions.map((a) => h(`button.btn.small${a.primary ? ".primary" : ""}`, { onclick: () => { a.run && a.run(); el.remove(); relayout(); } }, a.text)),
    h("button.iconbtn", { title: "Dismiss", html: icon("close", 14), onclick: () => { el.remove(); relayout(); } }));
  el.classList.add(kind);
  $("#notices").append(el);
  relayout();
  if (!sticky) setTimeout(() => { el.remove(); relayout(); }, 6000);
  return el;
}

let toastTimer;
function toast(text, ms = 2200) {
  const t = $("#toast");
  t.textContent = text;
  t.classList.add("on");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("on"), ms);
}

on("toast", (d) => {
  if (d.sticky) {
    const actions = [];
    if (d.parked) actions.push({ text: "Open it", primary: true, run: () => call("unpark", { id: d.parked }) });
    if (d.kind === "timer") actions.push({ text: "Pause timer", run: () => call("pomodoro.pause") }, { text: "Stop", run: () => call("pomodoro.stop") });
    notice(d.kind === "timer" ? "timer" : "info", d.kind === "timer" ? "timer" : "bell", d.text, actions);
  } else toast(d.text, d.ms);
});
on("nudge", (d) => {
  if ($("#notices .nudge")) return;
  const actions = d.kind === "tabs"
    ? [{ text: "Free memory", run: () => run("memory.free") }, { text: "Review tabs", run: () => run("palette") }]
    : [{ text: "Start a focus block", primary: true, run: () => call("pomodoro.start") }];
  notice("nudge", "bell", d.text, actions);
});
on("permission", (p) => {
  const el = notice("info", "shield", `${hostOf(p.origin) || p.origin} wants to use your ${p.perms.join(", ")}.`, [
    { text: "Block", run: () => call("permission.respond", { id: p.id, allow: false }) },
    { text: "Allow", primary: true, run: () => call("permission.respond", { id: p.id, allow: true }) },
  ]);
  el.dataset.perm = p.id;
});
on("permissionDone", (p) => { const el = $(`[data-perm="${p.id}"]`); if (el) { el.remove(); relayout(); } });
on("copy", (text) => { navigator.clipboard.writeText(text).then(() => toast("Link copied")).catch(() => {}); });
on("fullscreen", (fs) => { body.classList.toggle("fullscreen", !!fs); relayout(); });
on("focusStart", () => setTimeout(() => { const q = $("#start .w-search input"); if (q) q.focus(); else run("palette"); }, 40));

// ------------------------------------------------------------------ boot
on("state", (st) => { S = st; scheduleRender(); });
on("focus", (f) => { if (S) { S.focus = f; renderFoot(); } });
liveTheme();
call("ready").then((st) => {
  S = st;
  render();
  if (!st.settings.first_run_done) call("open", { url: "emerald://welcome/" });
}).catch((e) => console.error("ready failed", e));
