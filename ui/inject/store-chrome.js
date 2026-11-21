// Chrome Web Store: relabel the install buttons. Chrome's own installer still does the
// work (it runs inside Emerald), so extensions keep their real IDs and auto-update.
(() => {
  if (window.__emeraldStore) return;
  window.__emeraldStore = true;
  const MAP = [
    [/^Add to Chrome$/i, "Add to Emerald"],
    [/^Remove from Chrome$/i, "Remove from Emerald"],
    [/^Install Chrome$/i, "Emerald is a Chromium browser"],
  ];
  const fix = (root) => {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const t = n.nodeValue.trim();
      for (const [re, rep] of MAP) if (re.test(t)) n.nodeValue = n.nodeValue.replace(t, rep);
    }
    // Hide "Switch to Chrome?" upsell cards.
    for (const el of root.querySelectorAll ? root.querySelectorAll("[role=dialog], aside, div") : []) {
      if (el.childElementCount < 12 && /Switch to Chrome\?/.test(el.textContent || "") && el.textContent.length < 400) el.style.display = "none";
    }
  };
  fix(document.body);
  // The store re-renders labels in place, so re-scan (throttled to one pass per frame).
  let queued = false;
  new MutationObserver(() => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => { queued = false; fix(document.body); });
  }).observe(document.body, { childList: true, subtree: true, characterData: true });
})();
