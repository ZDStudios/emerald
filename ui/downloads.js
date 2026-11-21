import { call, on, h, $, icon, fmtBytes, liveTheme } from "./lib.js";

const main = $("#main");
main.className = "page";
const list = h("div.card");
main.replaceChildren(h("h1", "Downloads"), h("p.lead", "Files are saved straight to your downloads folder, with no dialogs."),
  h("div.toolbar", h("span", { style: { flex: 1 } }), h("button.btn.small", { onclick: () => call("downloads.clear").then(load) }, "Clear finished")), list);

function draw(items) {
  if (!items.length) { list.replaceChildren(h("div.empty", h("div", { html: icon("download", 28) }), "No downloads this session.")); return; }
  list.replaceChildren(...items.map((d) => {
    const pct = d.total > 0 ? Math.round(d.received / d.total * 100) : 0;
    const status = d.state === "complete" ? fmtBytes(d.total || d.received) : d.state === "in_progress" ? `${fmtBytes(d.received)} of ${d.total > 0 ? fmtBytes(d.total) : "?"} · ${fmtBytes(d.speed)}/s` : d.state;
    const btns = d.state === "in_progress" ? [h("button.btn.small", { onclick: () => call("downloads.control", { id: d.id, action: "pause" }) }, "Pause"), h("button.btn.small.ghost", { onclick: () => call("downloads.control", { id: d.id, action: "cancel" }) }, "Cancel")]
      : d.state === "paused" ? [h("button.btn.small", { onclick: () => call("downloads.control", { id: d.id, action: "resume" }) }, "Resume")]
      : d.state === "complete" ? [h("button.btn.small", { onclick: () => call("downloads.open", { path: d.path }) }, "Open"), h("button.btn.small.ghost", { onclick: () => call("downloads.open", { path: d.path, reveal: true }) }, "Show in folder")] : [];
    return h("div.dl", h("span", { html: icon(d.state === "complete" ? "check" : "download", 20) }),
      h("div.grow", h("div.nm", d.name), h("div.muted", { style: { fontSize: "12px" } }, status), d.state === "in_progress" ? h("div.meter", h("i", { style: { width: pct + "%" } })) : null), ...btns);
  }));
}
const load = () => call("downloads.list").then(draw);
on("downloads", draw);
liveTheme();
load();
