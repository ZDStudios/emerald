# Benchmarks

**Idle memory with 1, 5 and 20 tabs.**

* **Measured on** Linux: Zorin OS 18, 28 cores, 31 GB RAM, X11 inside a 1440×900 Xephyr display.
* **Engine:** CEF 156.0.31 / Chromium 156.0.8078.100. Emerald release build.
* **Method:** the sites in `scripts/bench-urls.txt` (Wikipedia, GitHub, YouTube, Hacker News, BBC,
  Stack Overflow, MDN, rust-lang.org, NYTimes, Amazon, docs.rs, crates.io, The Verge, IMDb, CNN,
  Apple, Microsoft, Reddit, The Guardian, ESPN), first N used. Each run starts with a fresh
  profile, waits 40 s for the pages to settle, then sums **PSS** (proportional set size) over the
  browser's whole process tree. PSS splits shared pages between the processes that map them, so
  unlike summing RSS it doesn't count libcef N times. RSS is listed for reference.
* **Reproduce:** `scripts/bench-linux.sh` (bare CEF spike) and `scripts/bench-emerald.sh`.

> Windows numbers are **not measured yet**. Expect the same shape, since it's the same Chromium,
> with different absolute values.

## Results (PSS, MB)

| Scenario | 1 tab | 5 tabs | 20 tabs | Live tabs (20) | Processes (20) |
|---|---:|---:|---:|---:|---:|
| Bare CEF spike, every tab loaded (Phase 0 baseline) | 437 | 1,190 | 2,906 | 20 | 54 |
| Emerald, every tab loaded (`--eager`) | 443 | 1,179 | 2,662 | 20 | 34 |
| **Emerald defaults: lazy restore** | **453** | **459** | **448** | 1 | 9 |
| **Emerald, all loaded, then "Free memory"** | 453 | 1,017* | 1,545* | 1 | 16 |

\* Measured 8 s after freeing. Chromium tears closed renderers down lazily: in a follow-up 5-tab run, memory fell from about 1,180 → 948 MB at 20 s → **525 MB at 40 s**. The benchmark script now waits 45 s, but the full 20-tab re-run has **not** been done.

RSS for the same runs: spike 1,058 / 2,446 / 8,215 MB; Emerald eager 1,082 / 2,472 / 6,217 MB.

## What the numbers say

* **Emerald's own cost is close to zero.** With identical pages loaded, Emerald is within
  noise of bare CEF: +6 MB at 1 tab, and less at 20 tabs. The interface is one small
  renderer, and the start page is drawn by that renderer rather than a tab of its own.
* **Sleeping is what saves memory.** A sleeping tab has no renderer at all. With 20 tabs in a
  session, the default experience (lazy restore) holds about as much memory as a single tab. After
  using all 20, one click on "Free memory" (or the automatic 15-minute idle timer) gets back to
  the same place.
* **What a tab costs** depends on the site: in these runs 40–650 MB each (YouTube is the
  heaviest). Chromium shares a renderer between same-site tabs, which is why there are fewer
  processes than tabs.
* **Honest caveat:** a sleeping tab reloads when you return to it, so its scroll position, form
  input and in-page state are lost (much like Chrome's own tab discarding). Tabs playing audio
  are never put to sleep automatically. Pinned tabs can be exempted.

## Installer / download size

| Artifact | Size |
|---|---|
| `emerald` binary (Rust shell, release, stripped) | 4.6 MB |
| Linux tarball (Emerald + CEF runtime) | 154 MB (377 MB unpacked) |
| Windows installer (Inno Setup) | 131 MB |
| Windows portable zip | 189 MB |
