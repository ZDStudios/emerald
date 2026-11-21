# Emerald architecture

Emerald is a minimal, customizable, ADHD-friendly desktop browser. Every line of code we
own (window and tab management, settings, import, extension glue, ADHD features, memory
manager) is Rust. The page renderer is Chromium, embedded through CEF.

## 1. Engine: Chromium, through CEF

**Why Chromium?** No Rust-native engine (Servo, Blitz, …) can render the modern web
reliably or run Chrome extensions. If you want "full Chrome/Edge extension support", the
engine has to be Chromium. Emerald is therefore *Chromium with a Rust shell*, not a Rust
engine.

**Version:** CEF 156.0.31 (Chromium 156.0.8078.100) through the
[`cef`](https://github.com/tauri-apps/cef-rs) crate `=156.7.0`, which ships generated Rust
bindings for the whole CEF C API.

### Phase 0 spike: results

Both spikes were meant to be run. Only the CEF spike could be run on the development
machine (Linux / Wayland). The reasons are listed below. `spike/cef-spike` is the code;
`scripts/bench-linux.sh` produced the numbers.

| Question | Rust + CEF (Chrome style) | Rust + wry / WebView2 |
|---|---|---|
| Heavy sites (YouTube, Wikipedia, GitHub, news sites) | ✅ verified | not run (see below) |
| uBlock Origin Lite | ✅ loads and blocks ad and tracker requests (verified with `fetch` probes) | ⚠️ documented as partial |
| Bitwarden | ✅ service worker runs; popup renders as a page | ⚠️ documented as partial |
| Dark Reader | ✅ verified restyling Wikipedia (26 injected style nodes, dark background) | ⚠️ documented as partial |
| Install from the Chrome Web Store | ✅ **native flow works**: `chrome.webstorePrivate` is present, "Add to Chrome" shows Chrome's confirm dialog, and the extension installs under its real store ID with auto-update | ❌ not supported (unpacked folders only) |
| Extension toolbar, popups, badges | provided by Emerald (see §4) | ❌ "missing UI entry points" (Microsoft docs) |
| Idle RAM, 1/5/20 tabs | see [BENCHMARKS.md](BENCHMARKS.md) | not run |
| Installer size | about 110–130 MB compressed on Windows (libcef.dll is the bulk) | about 3–5 MB (uses the system Edge WebView2 runtime) |

**Why the wry/WebView2 spike was not run.** WebView2 exists only on Windows. On Linux, wry
uses WebKitGTK, which is a different engine with zero Chrome-extension support, so
measuring it would say nothing about the Windows target. Microsoft's own documentation
settles the extension question:
`AddBrowserExtensionAsync` only takes an **unpacked folder**, it must be opted into with
`AreBrowserExtensionsEnabled`, and extensions with UI (icons, badges, popups) "will have
missing UI entry points." There is no Chrome Web Store install path.

**Decision: CEF.** Extensions were named as the deciding factor, and only CEF supports
them properly. WebView2's real advantages are a tiny installer and RAM shared with Edge
when Edge is already running. Those are traded away, and RAM is clawed back with tab
sleeping (§5).

### The constraint that shapes everything: Chrome style vs Alloy style

CEF has two runtime styles:

* **Chrome style** brings in the full Chrome extension system, the Web Store, the PDF
  viewer, print preview, the password manager and DevTools. This style is **required for extensions**.
* **Alloy style** is the bare content layer, with no extensions. It suits our own UI.

The spike surfaced a hard limit that CEF's headers also document: *"a Chrome style Window
can host at most one Chrome style BrowserView."* It was verified three ways:

| Attempt | Result |
|---|---|
| Two Chrome-style `BrowserView`s in one window | ❌ `Cannot add multiple Chrome style BrowserViews` |
| Remove one Chrome `BrowserView` and attach another (tab swap) | ❌ same error; the window stays bound to its first one |
| Alloy UI view first, then a Chrome content view | ❌ same error |
| Chrome content view first, then Alloy UI views | ✅ works, but only one tab |
| **Chrome-style browsers created as *native child windows* of the Emerald window** | ✅ **any number per window; show and hide per tab; extensions work** |

So Emerald hosts web content as **native child windows** (an `HWND` child on Windows, an X11
child on Linux) parented to a CEF Views top-level window. That window holds only Alloy-style
UI views.

## 2. Process and window layout

```
Emerald window (CEF Views, Alloy style, frameless, our draggable regions)
├── UI BrowserView (Alloy)  emerald://ui/shell.html   ← sidebar, top bar, start page chrome
│      draws everything except the content rectangle, which it reports to Rust
├── tab 1  Chrome-style browser, native child, mapped        ┐ positioned over the
├── tab 2  Chrome-style browser, native child, hidden        │ "content" rectangle
├── tab 3  sleeping: no browser, only metadata               ┘ (split view = two mapped)
└── overlay (Alloy, native child, raised)  emerald://ui/overlay.html
       command bar (Ctrl+K), extension popups host, memory meter, notes, nudges
```

* A native child window always paints above the Views-drawn UI. So anything that must
  float over web content (the command bar, menus, toasts) lives in a separate **overlay**
  native child that is raised above the tabs. The overlay is opaque, because X11 and Win32
  child windows don't alpha-blend with their siblings.
* **Extension-created tabs.** `chrome.tabs.create` and Chrome-internal "new tab" requests
  make CEF open a stock Chrome window. Emerald gets these through
  `BrowserProcessHandler::default_client`, notices a browser it did not create, closes it,
  and reopens the URL as an Emerald tab. This was verified in the spike with the Bitwarden
  and Dark Reader welcome tabs. You may briefly see a frame of the Chrome window flash.
* **Popups** (`window.open`): `on_before_popup` re-parents the popup as a native child tab,
  so `window.opener` survives (needed for OAuth). Real popup windows (`NEW_POPUP`
  disposition with features) keep CEF's small popup window.
* **Chromium's own session restore is disabled.** Each native-child tab is a Chrome
  "browser", so Chromium would reopen all of them after a restart, as extra windows.
  Emerald deletes the engine profile's `Sessions` files before startup and passes
  `--no-startup-window`. `session.json` is the only source of truth (testing found the
  duplication before this fix).
* **Single instance.** A second launch (for example a link clicked in another app while
  Emerald is the default browser) is forwarded by Chromium's process singleton to
  `on_already_running_app_relaunch`, which opens the URLs as tabs.
* **Re-entrancy.** Some CEF calls (such as `load_url`) can run handlers synchronously. Shell
  state lives in a thread-local `RefCell`; callbacks go through `shell::with`, which defers to
  the next loop turn when the shell is busy. The focus blocker additionally keeps a
  borrow-free snapshot, because a missed check there would let a blocked site through.

## 3. UI technology: HTML/CSS/JS in Alloy views (no Node, no build step)

Options considered: a native Rust UI (egui, iced, Slint) or a small web UI rendered by the
engine.

**Chosen: the web UI.** The reasons:

* **Theming is the product.** Themes are JSON that map onto CSS custom properties. A live
  editor, custom CSS injection, fonts, corner radius and density are all one stylesheet
  away. Getting the same reach in egui or iced would take a bespoke style system, and
  "custom CSS for the browser UI" would be impossible.
* **Composition.** A native Rust UI would have to share a window with CEF's native child
  windows and GPU surfaces, and handle IME, accessibility and HiDPI for both sides. The
  Alloy view gets all of that from Chromium for free.
* **Cost.** About 25–40 MB for one extra renderer process, measured in BENCHMARKS.md. The
  UI is plain ES modules served from memory through a custom `emerald://` scheme, with no
  framework and no build step. It is written in JavaScript rather than TypeScript, so
  building Emerald needs no Node toolchain at all.

**UI ⇄ Rust IPC**

* UI → Rust: `fetch("emerald://api/<cmd>", {method: "POST", headers: {"X-Emerald": "1"}, body})`.
  The scheme handler only accepts a request when the requesting frame's URL is itself
  `emerald://…`, the method is POST, and the custom header is present. Cross-origin
  pages can't attach the header without a CORS preflight, which is always refused.
* Rust → UI: `frame.execute_java_script("emerald.dispatch(…)")` pushes events (tab
  updates, memory samples, timers).

## 4. Extensions

| Piece | How |
|---|---|
| Install from the Chrome Web Store | Native Chrome flow (verified). Emerald injects a small script into `chromewebstore.google.com` that relabels "Add to Chrome" to **"Add to Emerald"** and leaves Chrome's installer to do the rest: real store ID and automatic updates. |
| Install from Edge Add-ons | The Edge store's install API exists only in Edge. Emerald relabels the button to "Add to Emerald", downloads the CRX from Microsoft's public update endpoint, verifies the CRX3 header, unpacks it, injects the public key as `manifest.key` so the extension ID matches Edge's, then loads it (see below). |
| Manage (list, enable, disable, remove, details) | A hidden Chrome-style browser on `chrome://extensions` exposes `chrome.developerPrivate` (verified: it lists all extensions with their state and install location). Emerald calls it and reads results back through console messages. Emerald's own manager UI is drawn in `emerald://extensions`. |
| Toolbar, pinning, popups | Emerald reads each extension's manifest (`action.default_icon` / `default_popup`), draws pinned icons in its toolbar, and opens the popup page in a Chrome-style native child anchored under the icon. Extension APIs work there because it is a Chrome-style browser. |
| MV2 | Chromium 156 has removed MV2 support. Emerald can't bring it back. Shown as "not possible". |

## 5. Memory strategy

* **Tab sleeping.** After a configurable idle time a tab's browser is *closed* and only
  its metadata (URL, title, favicon, scroll position) is kept. Clicking it recreates the
  browser. This is a real discard, since the renderer goes away. Sleeping tabs are dimmed
  with a moon badge. Pinned tabs, tabs playing audio and tabs with unsent form input can
  be exempted.
* **Lazy restore.** On startup only the active tab of each window is created; every other
  restored tab starts out asleep.
* **Process limit.** `--renderer-process-limit` (configurable) and process-per-site for
  same-origin tabs.
* **Memory meter.** CEF's `TaskManager` API (`task_id_for_browser_id`, `TaskInfo.memory`)
  gives per-tab memory without guessing PIDs. The "Free memory" button puts every
  inactive tab to sleep.
* **Main process stays small.** No polling loops. Timers are CEF delayed tasks. The UI is
  one renderer.

## 6. Crates and directories

```
crates/emerald        the browser (CEF app, windows, tabs, scheme/API, platform glue)
crates/emerald-core   pure Rust, unit-tested without CEF: settings, session/tab model,
                      themes, importers (Chrome/Edge profile + HTML/CSV), CRX tools,
                      site blocker and schedules, vault crypto, fuzzy search
crates/emerald-probe  dev harness: drives a running build over DevTools, measures RAM
ui/                   shell, overlay, start page, settings, wizard, …  (embedded at build)
themes/               built-in theme JSON
spike/cef-spike       the Phase 0 spike (kept for reproducibility)
scripts/              benchmarks
packaging/            Windows installer (Inno Setup) and Linux bundle scripts
```

## 7. Platform notes

* **Windows (primary target):** native child = `HWND` with `WS_CHILD`; show and hide with
  `ShowWindow` and `SetWindowPos`. The sandbox uses cef-rs's `bootstrap.exe` scheme.
* **Linux:** X11 child windows (`--ozone-platform=x11`, which also works under XWayland).
  Native Wayland can't host foreign child surfaces, so Emerald forces X11 there.
* **macOS:** child `NSView`s. The code paths are stubbed and gated, but not built or tested yet.
