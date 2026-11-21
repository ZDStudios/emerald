import { call, h, $, icon, favicon, prettyUrl, debounce, liveTheme } from "./lib.js";

const main = $("#main");
main.className = "page";
const q = h("input.input", { placeholder: "Search history", type: "search", style: { maxWidth: "380px" } });
const list = h("div.card", { style: { padding: "6px" } });
main.replaceChildren(
  h("h1", "History"), h("p.lead", "Stored only on this computer."),
  h("div.toolbar", q, h("span", { style: { flex: 1 } }),
    h("button.btn.small", { onclick: () => call("history.clear", { since: Date.now() - 3600e3 }).then(load) }, "Clear last hour"),
    h("button.btn.small.danger", { onclick: () => call("history.clear", { since: 0 }).then(load) }, "Clear all")),
  list);

async function load() {
  const items = await call("history.list", { q: q.value });
  if (!items.length) { list.replaceChildren(h("div.empty", q.value ? "No matches." : "Nothing here yet.")); return; }
  const out = [];
  let day = "";
  for (const e of items) {
    const d = new Date(e.last).toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });
    if (d !== day) { day = d; out.push(h("div.day-h", { style: { padding: "0 12px" } }, d)); }
    out.push(h("div.list-item", favicon({ url: e.url }, 16),
      h("span.muted", { style: { width: "56px", fontSize: "12px" } }, new Date(e.last).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })),
      h("a.t", { href: e.url, title: e.url }, e.title || prettyUrl(e.url)),
      h("span.u", { style: { maxWidth: "260px" } }, prettyUrl(e.url)),
      h("button.iconbtn", { title: "Remove from history", html: icon("trash", 15), onclick: () => call("history.delete", { url: e.url }).then(load) })));
  }
  list.replaceChildren(...out);
}
q.addEventListener("input", debounce(load, 150));
liveTheme();
load();
