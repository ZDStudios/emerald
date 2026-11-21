// Shown instead of a distracting site during focus time. Calm, never shaming.
import { call, h, $, icon, hostOf, fmtClock, liveTheme } from "./lib.js";

const p = new URLSearchParams(location.search);
const url = p.get("url") || "";
const reason = p.get("reason") || "Focus time";
const until = Number(p.get("until") || 0);
const site = hostOf(url) || "This site";

const timer = h("div.pill.accent", { style: { fontSize: "14px", padding: "4px 14px" } });
const tick = () => { timer.textContent = until > Date.now() ? `${reason} · ${fmtClock(until - Date.now())} left` : reason; };
tick();
setInterval(tick, 1000);

const main = $("#main");
main.className = "center-page";
main.replaceChildren(h("div.center-card",
  h("div.big-icon", { html: icon("leaf", 46) }),
  h("h1", `${site} can wait`),
  h("p", "You asked Emerald to keep distractions away right now. Take a breath. What were you about to work on?"),
  timer,
  h("div.acts",
    h("button.btn.primary", { onclick: () => history.length > 1 ? history.back() : call("cmd", { name: "tab.close" }) }, h("span", { html: icon("back", 15) }), "Back to what I was doing"),
    h("button.btn", { onclick: () => call("cmd", { name: "note.quick" }) }, h("span", { html: icon("note", 15) }), "Jot down the thought"),
    h("button.btn.ghost", { title: "Opens the site for 5 minutes", onclick: () => call("focus.allowOnce", { url, minutes: 5 }) }, "Allow 5 minutes")),
));
document.title = `${site}: focus time`;
liveTheme();
