// Theme editor: live preview of every theme value, save as your own, import/export packs.
import { call, h, $, icon, liveTheme, debounce } from "./lib.js";

const main = $("#main");
main.className = "page";
main.style.maxWidth = "1100px";
let themes = [], settings = null, draft = null;

const LABELS = {
  bg: "Background", surface: "Surface (sidebar, cards)", "surface-2": "Raised surface", hover: "Hover", active: "Selected", border: "Borders",
  text: "Text", "text-muted": "Muted text", accent: "Accent", "accent-text": "Text on accent", "accent-soft": "Soft accent (highlights)",
  danger: "Danger", warn: "Warning", success: "Success", "focus-ring": "Keyboard focus ring", shadow: "Shadow",
};
const FONTS = [
  ['"Inter", "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif', "System sans (default)"],
  ['"Atkinson Hyperlegible", Verdana, sans-serif', "Atkinson Hyperlegible / Verdana (very legible)"],
  ['"OpenDyslexic", Verdana, sans-serif', "OpenDyslexic (dyslexia-friendly)"],
  ['Georgia, "Iowan Old Style", serif', "Serif"],
  ['"JetBrains Mono", "Cascadia Code", Consolas, monospace', "Monospace"],
  ['"Comic Sans MS", "Comic Neue", cursive', "Comic (friendly, legible)"],
];

// Colors may be rgba()/hsl(); <input type=color> needs #rrggbb, so convert when possible.
function toHex(v) {
  const c = document.createElement("canvas").getContext("2d");
  c.fillStyle = "#000"; c.fillStyle = v;
  return /^#[0-9a-f]{6}$/i.test(c.fillStyle) ? c.fillStyle : "#000000";
}

const previewStyle = h("style");
document.head.append(previewStyle);
const preview = debounce(async () => {
  try { previewStyle.textContent = (await call("theme.css", { theme: draft })).replace(":root{", ".pv-scope{"); } catch (e) { status(e.message, true); }
}, 80);

function status(text, err) { const s = $("#tstatus"); if (s) { s.textContent = text; s.style.color = err ? "var(--em-danger)" : ""; } }

function editor() {
  const field = (label, input) => h("label.tf", h("span", label), input);
  const colors = h("div.colors", ...Object.keys(LABELS).map((k) => {
    const val = draft.colors[k] || "";
    const txt = h("input.input", { value: val, spellcheck: "false" });
    const pick = h("input", { type: "color", value: toHex(val) });
    pick.oninput = () => { txt.value = pick.value; draft.colors[k] = pick.value; preview(); };
    txt.oninput = () => { draft.colors[k] = txt.value; pick.value = toHex(txt.value); preview(); };
    return h("div.crow", pick, h("div.cl", h("div", LABELS[k]), txt));
  }));
  const name = h("input.input", { value: draft.name, oninput: (e) => (draft.name = e.target.value) });
  const font = h("select.input", { onchange: (e) => { draft.font_ui = e.target.value; preview(); } }, ...FONTS.map(([v, t]) => h("option", { value: v, selected: draft.font_ui === v }, t)));
  const radius = h("input", { type: "range", min: 0, max: 24, value: draft.radius, oninput: (e) => { draft.radius = Number(e.target.value); preview(); } });
  const mode = h("select.input", { onchange: (e) => { draft.mode = e.target.value; preview(); } }, h("option", { value: "dark", selected: draft.mode === "dark" }, "Dark"), h("option", { value: "light", selected: draft.mode === "light" }, "Light"));
  const density = h("select.input", { onchange: (e) => { draft.density = e.target.value; preview(); } }, ...["compact", "comfortable", "spacious"].map((d) => h("option", { value: d, selected: draft.density === d }, d[0].toUpperCase() + d.slice(1))));
  const layout = h("select.input", { onchange: (e) => { draft.tab_layout = e.target.value || null; } }, h("option", { value: "", selected: !draft.tab_layout }, "Keep my layout"), h("option", { value: "vertical", selected: draft.tab_layout === "vertical" }, "Vertical tabs"), h("option", { value: "horizontal", selected: draft.tab_layout === "horizontal" }, "Horizontal tabs"));
  const startBg = h("input.input", { value: draft.start_background, placeholder: "CSS background for the start page", oninput: (e) => { draft.start_background = e.target.value; preview(); } });
  const css = h("textarea.input.code", { spellcheck: "false", placeholder: "/* Extra CSS shipped with this theme */" }, draft.css);
  css.oninput = () => { draft.css = css.value; preview(); };
  return h("div.ed",
    h("div.card.pad", h("h3", "Basics"), field("Name", name), field("Mode", mode), field("Font", font), field(`Corner radius`, radius), field("Density", density), field("Tab layout", layout), field("Start page background", startBg)),
    h("div.card.pad", h("h3", "Colors"), colors),
    h("div.card.pad", h("h3", "Theme CSS"), css));
}

function previewPane() {
  return h("div.pv-scope.pvw",
    h("div.pv-win",
      h("div.pv-sb", h("div.pv-sp", "🌿 Home"), h("div.pv-new", "+ New tab"),
        h("div.pv-tab.on", "● Emerald – Wikipedia"), h("div.pv-tab", "● GitHub"), h("div.pv-tab.sl", "☾ Sleeping tab"), h("div.pv-fold", "▸ Research")),
      h("div.pv-main", h("div.pv-top", h("span", "‹ › ⟳"), h("div.pv-addr", "🔒 en.wikipedia.org/wiki/Emerald")),
        h("div.pv-page", h("div.pv-h", "Good evening"), h("div.pv-search", "Search or type a URL"),
          h("div.pv-cards", h("div.pv-card", h("b", "Focus"), h("div.pv-big", "25:00"), h("button.pv-btn", "Start focus")), h("div.pv-card", h("b", "Quick notes"), h("div.pv-muted", "Call the dentist"), h("div.pv-muted", "Read later list"))),
          h("div.pv-row", h("span.pv-pill.ok", "Saved"), h("span.pv-pill.warn", "Nudge"), h("span.pv-pill.bad", "Blocked"))))));
}

function render() {
  const list = h("div.tlist", ...themes.map((t) => {
    const b = h("button.tl", { onclick: () => { draft = structuredClone(t); if (t.builtin) { draft.id = t.id + "-custom"; draft.name = t.name + " (custom)"; } render(); } },
      h("span.sw", { style: { background: `linear-gradient(135deg, ${t.colors.bg} 50%, ${t.colors.accent} 50%)` } }), h("span", t.name), t.builtin ? h("span.muted", "built-in") : h("span.pill", "yours"));
    if (draft && (t.id === draft.id || t.id + "-custom" === draft.id)) b.classList.add("on");
    return b;
  }));
  const importInput = h("input", { type: "file", accept: ".json", hidden: true, onchange: async () => {
    const f = importInput.files[0];
    if (!f) return;
    try { const ids = await call("theme.import", { json: await f.text() }); status(`Imported ${ids.length} theme${ids.length === 1 ? "" : "s"}: ${ids.join(", ")}`); await load(); } catch (e) { status(e.message, true); }
  } });
  const exportPack = async () => {
    const mine = themes.filter((t) => !t.builtin).map((t) => t.id);
    const ids = mine.length ? mine : [settings.theme];
    const json = await call("theme.export", { ids, name: "My Emerald themes" });
    const a = h("a", { href: URL.createObjectURL(new Blob([json], { type: "application/json" })), download: "emerald-themes.json" });
    document.body.append(a); a.click(); a.remove();
    status(`Exported ${ids.length} theme${ids.length === 1 ? "" : "s"} to your downloads folder.`);
  };
  main.replaceChildren(
    h("h1", "Theme editor"),
    h("p.lead", "Changes preview live on the right. Save to apply it to the whole browser. Built-in themes are copied when you edit them."),
    h("div.toolbar",
      h("button.btn.primary", { onclick: async () => { try { const id = await call("theme.save", { theme: draft, apply: true }); status(`Saved and applied “${draft.name}”`); await load(id); } catch (e) { status(e.message, true); } } }, h("span", { html: icon("check", 15) }), "Save & apply"),
      h("button.btn", { onclick: () => importInput.click() }, h("span", { html: icon("import", 15) }), "Import pack", importInput),
      h("button.btn", { onclick: exportPack }, h("span", { html: icon("download", 15) }), "Export my themes"),
      !draft.builtin && themes.some((t) => t.id === draft.id && !t.builtin) ? h("button.btn.ghost.danger", { onclick: async () => { await call("theme.delete", { id: draft.id }); await load(); } }, "Delete") : null,
      h("span#tstatus.muted")),
    h("div.layout", h("div.left", list, editor()), h("div.right", previewPane())));
  preview();
}

async function load(selectId) {
  [themes, settings] = await Promise.all([call("themes.list"), call("settings.get")]);
  const cur = themes.find((t) => t.id === (selectId || settings.theme)) || themes[0];
  draft = structuredClone(cur);
  if (cur.builtin) { draft.id = cur.id + "-custom"; draft.name = cur.name + " (custom)"; }
  render();
}

document.head.append(h("style", `
  .layout { display: grid; grid-template-columns: minmax(360px, 1fr) minmax(360px, 1.1fr); gap: 20px; align-items: start; }
  .right { position: sticky; top: 20px; }
  .tlist { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 14px; }
  .tl { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: 99px; border: 1px solid var(--em-border); background: var(--em-surface); cursor: pointer; font-size: 12.5px; }
  .tl.on { border-color: var(--em-accent); background: var(--em-accent-soft); }
  .tl .sw { width: 16px; height: 16px; border-radius: 50%; border: 1px solid var(--em-border); }
  .ed { display: grid; gap: 14px; } .pad { padding: 14px 16px; } .ed h3 { margin: 0 0 10px; font-size: 13px; }
  .tf { display: grid; grid-template-columns: 150px 1fr; gap: 10px; align-items: center; margin-bottom: 8px; font-size: 12.5px; color: var(--em-text-muted); }
  .colors { display: grid; grid-template-columns: 1fr 1fr; gap: 8px 14px; }
  .crow { display: flex; gap: 8px; align-items: center; } .crow input[type=color] { width: 34px; height: 34px; border: 0; padding: 0; background: none; cursor: pointer; flex: none; }
  .cl { flex: 1; min-width: 0; font-size: 11.5px; color: var(--em-text-muted); } .cl .input { min-height: 26px; padding: 2px 8px; font: 12px var(--em-mono); }
  /* Preview uses only the draft's variables (scoped via .pv-scope). */
  .pvw { border-radius: 14px; overflow: hidden; border: 1px solid var(--em-border); box-shadow: 0 20px 50px rgba(0,0,0,.25); font-family: var(--em-font); }
  .pv-win { display: flex; height: 440px; background: var(--em-bg); color: var(--em-text); font-size: 12.5px; }
  .pv-sb { width: 34%; background: var(--em-surface); border-right: 1px solid var(--em-border); padding: 8px; display: grid; align-content: start; gap: 4px; }
  .pv-sp { padding: 4px 10px; border-radius: 99px; background: var(--em-accent-soft); justify-self: start; }
  .pv-new { padding: 7px 10px; border: 1px dashed var(--em-border); border-radius: var(--em-radius-sm); color: var(--em-text-muted); }
  .pv-tab { padding: calc(var(--em-pad) + 1px) 10px; border-radius: var(--em-radius-sm); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pv-tab.on { background: var(--em-active); box-shadow: inset 3px 0 0 var(--em-accent); } .pv-tab.sl { color: var(--em-text-muted); }
  .pv-fold { padding: 6px 10px; color: var(--em-text-muted); font-weight: 600; }
  .pv-main { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .pv-top { display: flex; gap: 10px; align-items: center; padding: 8px 10px; border-bottom: 1px solid var(--em-border); color: var(--em-text-muted); }
  .pv-addr { flex: 1; padding: 5px 12px; border-radius: 99px; border: 1px solid var(--em-border); background: var(--em-surface); color: var(--em-text); }
  .pv-page { flex: 1; padding: 20px; background: var(--em-start-bg, var(--em-bg)); background-size: cover; display: grid; gap: 12px; align-content: start; }
  .pv-h { text-align: center; color: var(--em-text-muted); }
  .pv-search { padding: 12px 18px; border-radius: 99px; border: 1px solid var(--em-border); background: var(--em-surface); color: var(--em-text-muted); box-shadow: 0 6px 20px var(--em-shadow); }
  .pv-cards { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .pv-card { background: var(--em-surface); border: 1px solid var(--em-border); border-radius: var(--em-radius); padding: 12px; display: grid; gap: 6px; }
  .pv-big { font: 300 26px var(--em-mono); } .pv-muted { color: var(--em-text-muted); }
  .pv-btn { justify-self: start; border: 0; border-radius: var(--em-radius-sm); background: var(--em-accent); color: var(--em-accent-text); padding: 6px 12px; font-weight: 600; }
  .pv-row { display: flex; gap: 8px; } .pv-pill { padding: 2px 10px; border-radius: 99px; font-size: 11.5px; border: 1px solid currentColor; }
  .pv-pill.ok { color: var(--em-success); } .pv-pill.warn { color: var(--em-warn); } .pv-pill.bad { color: var(--em-danger); }
  @media (max-width: 900px) { .layout { grid-template-columns: 1fr; } .right { position: static; } }
`));
liveTheme();
load();
