// Emerald reader mode. Injected into the page; toggles a clean, distraction-free view.
// Content extraction: score block elements by paragraph text, penalise link-heavy and
// boilerplate-looking containers, keep the best candidate.
(() => {
  const KEY = "__emeraldReader";
  if (window[KEY]) {
    // Toggle off: restore the original page.
    document.documentElement.replaceWith(window[KEY]);
    window[KEY] = null;
    return;
  }
  const NEG = /comment|meta|footer|footnote|sidebar|sponsor|promo|related|share|social|nav|menu|header|banner|ad-|ads|advert|cookie|consent|newsletter|subscribe|popup|modal|breadcrumb|reflist|references|citation|cite|bibliograph|navbox|infobox|toc\b/i;
  const POS = /article|body|content|entry|main|page|post|text|blog|story|prose|markdown/i;
  const textLen = (el) => (el.innerText || "").trim().length;
  const linkDensity = (el) => {
    const t = textLen(el) || 1;
    let l = 0;
    el.querySelectorAll("a").forEach((a) => (l += textLen(a)));
    return l / t;
  };
  const scores = new Map();
  document.querySelectorAll("p, pre, blockquote, li").forEach((p) => {
    const len = textLen(p);
    if (len < 40) return;
    // Paragraphs carry the article; list items (often citations or link lists) count far less.
    const weight = p.tagName === "LI" ? 0.2 : 1;
    const s = weight * (1 + Math.min(Math.floor(len / 100), 4) + (p.innerText.split(",").length - 1) * 0.5);
    let el = p.parentElement, depth = 0;
    while (el && el !== document.body && depth < 4) {
      scores.set(el, (scores.get(el) || 0) + s / (depth + 1));
      el = el.parentElement;
      depth++;
    }
  });
  let best = null, bestScore = 0;
  for (const [el, s0] of scores) {
    let s = s0;
    const hint = (el.className || "") + " " + (el.id || "");
    if (typeof hint === "string") {
      if (POS.test(hint)) s *= 1.4;
      if (NEG.test(hint)) s *= 0.3;
    }
    if (/^(ARTICLE|MAIN)$/.test(el.tagName)) s *= 1.5;
    s *= 1 - Math.min(linkDensity(el), 0.9);
    if (s > bestScore) { bestScore = s; best = el; }
  }
  best = best || document.querySelector("article, main") || document.body;
  const clone = best.cloneNode(true);
  clone.querySelectorAll("script, style, noscript, iframe, form, button, input, select, textarea, nav, aside, footer, svg, canvas, [aria-hidden=true], [role=navigation], [role=complementary]").forEach((n) => n.remove());
  clone.querySelectorAll("*").forEach((n) => {
    const hint = (n.className && typeof n.className === "string" ? n.className : "") + " " + (n.id || "");
    if (NEG.test(hint) && textLen(n) < 600 && n.tagName !== "P") { n.remove(); return; }
    for (const a of [...n.attributes]) {
      if (!["href", "src", "alt", "title", "srcset", "colspan", "rowspan"].includes(a.name)) n.removeAttribute(a.name);
    }
    if (n.tagName === "IMG" && !n.getAttribute("src")) {
      const ds = best.querySelector(`img[alt="${(n.alt || "").replace(/"/g, "")}"]`);
      if (ds && ds.dataset && ds.dataset.src) n.src = ds.dataset.src;
    }
    if (n.tagName === "A") n.setAttribute("target", "_blank");
  });
  const title = (document.querySelector("h1")?.innerText || document.title || "").trim();
  clone.querySelectorAll("h1").forEach((h) => { if (h.innerText.trim() === title) h.remove(); });
  const words = (clone.innerText || "").split(/\s+/).filter(Boolean).length;
  const site = location.hostname.replace(/^www\./, "");
  const byline = document.querySelector('[rel=author], .author, [itemprop=author], meta[name=author]');
  const author = byline ? (byline.content || byline.innerText || "").trim().slice(0, 80) : "";

  window[KEY] = document.documentElement;
  const html = document.createElement("html");
  html.innerHTML = `<head><meta charset="utf-8"><title></title><style>
    :root{--bg:#fbfaf6;--fg:#24211c;--muted:#7b7468;--accent:#178f5c;--size:20px;--width:68ch;--font:Charter,"Iowan Old Style",Georgia,serif;color-scheme:light dark}
    :root.dark{--bg:#121614;--fg:#dfe6e2;--muted:#8a978f;--accent:#4fcf8f;color-scheme:dark}
    :root.light{color-scheme:light}
    :root.sans{--font:"Inter","Segoe UI",system-ui,sans-serif}
    :root.dys{--font:"OpenDyslexic","Atkinson Hyperlegible",Verdana,sans-serif;letter-spacing:.02em;word-spacing:.12em}
    html,body{background:var(--bg);color:var(--fg);margin:0}
    body{font:var(--size)/1.75 var(--font);padding:72px 24px 120px}
    article{max-width:var(--width);margin:0 auto}
    h1.t{font-size:2em;line-height:1.2;margin:0 0 .4em}
    .meta{color:var(--muted);font:14px/1.4 "Inter",system-ui,sans-serif;margin-bottom:2.2em}
    img,video,figure{max-width:100%;height:auto;border-radius:8px}
    a{color:var(--accent)} pre{overflow:auto;padding:14px;border-radius:8px;background:rgba(127,127,127,.12);font-size:.8em}
    blockquote{margin:1em 0;padding-left:1em;border-left:3px solid var(--accent);color:var(--muted)}
    .bar{position:fixed;top:12px;right:16px;display:flex;gap:6px;font:13px system-ui,sans-serif;opacity:.55;transition:opacity .2s}
    .bar:hover{opacity:1} .bar button{min-width:36px;height:32px;border-radius:8px;border:1px solid rgba(127,127,127,.35);background:transparent;color:var(--fg);cursor:pointer}
  </style></head><body><div class="bar">
    <button data-a="smaller" title="Smaller text">A−</button><button data-a="bigger" title="Larger text">A+</button>
    <button data-a="font" title="Serif / sans">Aa</button><button data-a="dys" title="Dyslexia-friendly font">Dy</button>
    <button data-a="narrow" title="Narrower">⇥⇤</button><button data-a="wide" title="Wider">⇤⇥</button>
    <button data-a="close" title="Exit reader (Ctrl+Shift+R)">✕</button></div>
    <article><h1 class="t"></h1><div class="meta"></div><div class="c"></div></article></body>`;
  html.querySelector("title").textContent = title;
  html.querySelector("h1.t").textContent = title;
  html.querySelector(".meta").textContent = [site, author, `${Math.max(1, Math.round(words / 230))} min read`].filter(Boolean).join(" · ");
  html.querySelector(".c").append(...clone.childNodes);
  document.documentElement.replaceWith(html);
  const root = document.documentElement;
  root.classList.add(window.__emReaderLight ? "light" : "dark");
  if (window.__emDysFont) {
    try { const f = new FontFace("OpenDyslexic", `url(${window.__emDysFont})`); f.load().then((x) => document.fonts.add(x)); } catch {}
    root.classList.add("dys");
  }
  let size = 20, width = 68;
  document.querySelector(".bar").addEventListener("click", (e) => {
    const a = e.target.dataset && e.target.dataset.a;
    if (a === "bigger") size = Math.min(32, size + 2);
    if (a === "smaller") size = Math.max(14, size - 2);
    if (a === "wide") width = Math.min(100, width + 8);
    if (a === "narrow") width = Math.max(44, width - 8);
    if (a === "font") root.classList.toggle("sans");
    if (a === "dys") root.classList.toggle("dys");
    if (a === "close") { document.documentElement.replaceWith(window[KEY]); window[KEY] = null; return; }
    root.style.setProperty("--size", size + "px");
    root.style.setProperty("--width", width + "ch");
  });
  window.scrollTo(0, 0);
})();
