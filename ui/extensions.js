// Extension manager: list, enable/disable, pin to toolbar, options, remove.
import { call, on, h, $, icon, liveTheme } from "./lib.js";

const main = $("#main");
main.className = "page";
const list = h("div");
const LOC = { FROM_STORE: "Chrome Web Store", UNPACKED: "Unpacked / Edge Add-ons", COMMAND_LINE: "Command line", INSTALLED_BY_DEFAULT: "Built in", THIRD_PARTY: "External", UNKNOWN: "" };

main.replaceChildren(
  h("h1", "Extensions"),
  h("p.lead", "Manifest V3 extensions from the Chrome Web Store and Edge Add-ons. Click “Add to Emerald” on any extension page."),
  h("div.toolbar",
    h("button.btn.primary", { onclick: () => call("open", { url: "https://chromewebstore.google.com/category/extensions" }) }, h("span", { html: icon("external", 15) }), "Chrome Web Store"),
    h("button.btn", { onclick: () => call("open", { url: "https://microsoftedge.microsoft.com/addons/Microsoft-Edge-Extensions-Home" }) }, h("span", { html: icon("external", 15) }), "Edge Add-ons"),
    h("span", { style: { flex: 1 } }),
    h("button.btn.ghost", { title: "Load an unpacked extension folder (for developers)", onclick: () => {
      const p = window.prompt ? null : null;
      const input = h("input.input", { placeholder: "/path/to/extension-folder", style: { maxWidth: "320px" } });
      const row = h("div.toolbar", input, h("button.btn.small", { onclick: () => call("ext.action", { do: "loadUnpacked", path: input.value }).catch((e) => alertRow(e.message)) }, "Load"));
      list.before(row); input.focus(); return p;
    } }, "Load unpacked…"),
    h("button.iconbtn", { title: "Refresh", html: icon("reload", 16), onclick: () => call("ext.action", { do: "refresh" }) })),
  list,
  h("div.card", { style: { padding: "14px 16px", marginTop: "24px" } },
    h("b", "What works"),
    h("p.muted", { style: { fontSize: "12.5px", lineHeight: 1.6 } },
      "Content scripts, background service workers, context menus, the store's install/remove, auto-updates (Chrome Web Store), and options pages all work. " +
      "Popups open in an Emerald panel under the toolbar icon. A few popups that act on “the current tab” (for example per-site toggles) may not see the page behind them; use the extension's keyboard shortcut or context-menu entry instead. " +
      "Manifest V2 extensions can't run: Chromium removed MV2 support.")));

function alertRow(text) { list.before(h("div.notice", text)); }

function draw(items) {
  if (!items || !items.length) {
    list.replaceChildren(h("div.empty", h("div", { html: icon("puzzle", 30) }), "No extensions yet. Try uBlock Origin Lite, Bitwarden or Dark Reader."));
    return;
  }
  list.replaceChildren(h("div.card", ...items.map((e) => {
    const toggle = h("input.switch", { type: "checkbox", checked: e.enabled, title: e.enabled ? "Disable" : "Enable", onchange: (ev) => call("ext.action", { do: ev.target.checked ? "enable" : "disable", id: e.id }) });
    const pin = h("button.iconbtn", { title: e.pinned ? "Unpin from toolbar" : "Pin to toolbar", html: icon("pin", 16), onclick: () => call("ext.action", { do: "pin", id: e.id, pinned: !e.pinned }) });
    if (e.pinned) pin.classList.add("on");
    const details = h("details", { style: { marginTop: "6px" } }, h("summary.muted", { style: { cursor: "pointer", fontSize: "12px" } }, "Details"),
      h("div.muted", { style: { fontSize: "12px", lineHeight: 1.6, paddingTop: "6px" } },
        h("div", `ID: ${e.id}`), h("div", `Source: ${LOC[e.location] || e.location || "?"} · Manifest V${e.mv || "?"}`),
        e.permissions && e.permissions.length ? h("div", "Can: " + e.permissions.join("; ")) : null));
    return h("div.row", { style: { alignItems: "flex-start" } },
      e.icon ? h("img", { src: e.icon, width: 32, height: 32, alt: "", style: { borderRadius: "6px", marginTop: "2px" } }) : h("span", { html: icon("puzzle", 32) }),
      h("div.grow", h("div.label", e.name, h("span.muted", { style: { fontWeight: 400, marginLeft: "8px", fontSize: "12px" } }, e.version)), h("div.desc", e.description), details),
      h("div.ctl",
        e.optionsUrl ? h("button.btn.small.ghost", { onclick: () => call("ext.action", { do: "options", url: e.optionsUrl }) }, "Options") : null,
        pin, toggle,
        h("button.iconbtn", { title: "Remove", html: icon("trash", 16), onclick: () => { if (confirmRemove(e)) call("ext.action", { do: "remove", id: e.id }); } })));
  })));
}

let armed = null;
function confirmRemove(e) {
  // Two-step remove without a modal dialog: first click arms, second click (within 4 s) removes.
  if (armed === e.id) { armed = null; return true; }
  armed = e.id;
  alertRow(`Click the trash icon again to remove ${e.name}.`);
  setTimeout(() => { if (armed === e.id) armed = null; }, 4000);
  return false;
}

on("extensions", draw);
liveTheme();
call("ext.list").then(draw);
call("ext.action", { do: "refresh" });
