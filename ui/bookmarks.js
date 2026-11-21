import { call, h, $, icon, favicon, prettyUrl, liveTheme } from "./lib.js";

const main = $("#main");
main.className = "page";
const q = h("input.input", { placeholder: "Filter bookmarks", type: "search", style: { maxWidth: "380px" } });
const list = h("div");
main.replaceChildren(h("h1", "Bookmarks"), h("p.lead", "Press Ctrl+D on any page to bookmark it. Bookmarks are searchable from the command bar (Ctrl+K)."), h("div.toolbar", q), list);

let all = [];
function draw() {
  const f = q.value.toLowerCase();
  const items = all.filter((b) => !f || b.title.toLowerCase().includes(f) || b.url.toLowerCase().includes(f));
  if (!items.length) { list.replaceChildren(h("div.empty", all.length ? "No matches." : "No bookmarks yet. Import them from Chrome or Edge in Settings → General.")); return; }
  const folders = new Map();
  for (const b of items) { const k = b.folder || "Bookmarks"; if (!folders.has(k)) folders.set(k, []); folders.get(k).push(b); }
  list.replaceChildren(...[...folders].map(([name, bs]) => h("div", { style: { marginBottom: "18px" } },
    h("div.day-h", h("span", { html: icon("folder", 13), style: { display: "inline-block", verticalAlign: "-2px", marginRight: "6px" } }), name, ` · ${bs.length}`),
    h("div.card", { style: { padding: "6px" } }, ...bs.map((b) => h("div.list-item", favicon({ url: b.url }, 16), h("a.t", { href: b.url, title: b.url }, b.title || prettyUrl(b.url)), h("span.u", { style: { maxWidth: "280px" } }, prettyUrl(b.url)),
      h("button.iconbtn", { title: "Delete", html: icon("trash", 15), onclick: async () => { all = await call("bookmarks.delete", { url: b.url }); draw(); } })))))));
}
q.addEventListener("input", draw);
liveTheme();
call("bookmarks.list").then((b) => { all = b; draw(); });
