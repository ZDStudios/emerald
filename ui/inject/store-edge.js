// Edge Add-ons: the store's install API only exists inside Edge, so its "Get" button
// can't work here. Emerald adds an "Add to Emerald" button that asks Emerald to download,
// verify and install the extension itself.
(() => {
  if (window.__emeraldEdge) return;
  window.__emeraldEdge = true;
  const idFromUrl = () => (location.pathname.match(/\/([a-p]{32})(?:[/?#]|$)/) || [])[1];
  const install = (id) => { location.href = "https://emerald.invalid/install?store=edge&id=" + id; };
  const style = document.createElement("style");
  style.textContent = `.emerald-add{display:inline-flex;align-items:center;gap:8px;height:32px;padding:0 18px;border-radius:6px;border:0;
    background:#178f5c;color:#fff;font:600 14px/1 "Segoe UI",system-ui,sans-serif;cursor:pointer;margin:6px 0}
    .emerald-add:hover{background:#13784d}`;
  document.documentElement.appendChild(style);
  const decorate = () => {
    const id = idFromUrl();
    if (!id) return;
    const buttons = [...document.querySelectorAll("button")].filter((b) => /^(Get|Remove)$/i.test((b.innerText || "").trim()) && !b.dataset.emerald);
    for (const b of buttons) {
      b.dataset.emerald = "1";
      const add = document.createElement("button");
      add.className = "emerald-add";
      add.textContent = "Add to Emerald";
      add.addEventListener("click", (e) => { e.preventDefault(); e.stopPropagation(); install(id); });
      b.style.display = "none";
      b.insertAdjacentElement("afterend", add);
    }
  };
  decorate();
  new MutationObserver(decorate).observe(document.body, { childList: true, subtree: true });
})();
