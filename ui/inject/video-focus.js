// Emerald distraction-free video mode: show only the video, dim everything else.
// Works on YouTube (keeps its player controls) and on any page with a <video>.
(() => {
  const ID = "__emerald_video_focus";
  const existing = document.getElementById(ID);
  if (existing) { existing.remove(); document.documentElement.classList.remove(ID); return; }
  const yt = document.querySelector("#movie_player");
  let target = yt;
  if (!target) {
    const vids = [...document.querySelectorAll("video")].sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight);
    target = vids[0];
  }
  if (!target) { console.info("Emerald: no video on this page"); return; }
  target.setAttribute("data-emerald-focus", "1");
  const style = document.createElement("style");
  style.id = ID;
  style.textContent = `
    html.${ID}, html.${ID} body { overflow: hidden !important; background: #000 !important; }
    html.${ID} [data-emerald-focus] {
      position: fixed !important; inset: 0 !important; width: 100vw !important; height: 100vh !important;
      max-width: none !important; max-height: none !important; z-index: 2147483646 !important; background: #000 !important;
    }
    html.${ID} [data-emerald-focus] video { width: 100% !important; height: 100% !important; object-fit: contain !important; left: 0 !important; top: 0 !important; }
    /* YouTube: hide end-screen cards, suggestions overlays and comments while focused */
    html.${ID} .ytp-ce-element, html.${ID} .ytp-endscreen-content, html.${ID} .ytp-pause-overlay,
    html.${ID} #related, html.${ID} #comments, html.${ID} ytd-watch-next-secondary-results-renderer { display: none !important; }`;
  document.documentElement.appendChild(style);
  document.documentElement.classList.add(ID);
  window.dispatchEvent(new Event("resize"));
})();
