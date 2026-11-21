# Feature checklist

Legend: ✅ done and verified · 🟡 partial (see note) · ❌ not possible (with the closest alternative) · ⬜ not done yet

"Verified" means exercised in a running build on Linux/X11 (CEF 156, Chromium 156.0.8078.100),
using `emerald-probe` and screenshots. **Windows has not been run yet**: the Windows code paths
(native child windows, DPAPI, installer) are written and built by CI, but untested on real hardware.

## Engine & architecture

| | Status | Notes |
|---|---|---|
| Rust for all owned code (shell, tabs, settings, import, extensions glue, ADHD, memory) | ✅ | The UI is HTML/CSS/JS rendered by the engine (allowed by the brief; reasons in ARCHITECTURE.md). No Electron, no Node at build time or run time. |
| Phase 0 spike: Rust + CEF | ✅ | `spike/cef-spike`: heavy sites, 4 extensions, store install, RAM 1/5/20 tabs. |
| Phase 0 spike: Rust + wry/WebView2 | ❌ | WebView2 is Windows-only, and wry on Linux is WebKitGTK (not Chromium). Evaluated from Microsoft's docs instead: unpacked extensions only, no Web Store, "missing UI entry points". CEF was chosen. |
| Windows first, cross-platform-ready | 🟡 | Linux works (X11/XWayland). Windows and macOS (Apple Silicon `.app`) are compiled by CI but haven't been run on real machines yet. |

## Core

| | Status | Notes |
|---|---|---|
| Thin top bar + compact sidebar | ✅ | |
| Everything auto-hideable | ✅ | Sidebar and top bar auto-hide (reveal on edge hover), focus mode, full screen. Revealing *pushes* the page rather than covering it, because native page windows always draw on top. |
| No clutter, no bundled junk, no telemetry | ✅ | Chromium components still update themselves (extension updates and similar). |
| Tab sleeping after configurable idle time, visual state | ✅ | Default 15 min. Sleeping tabs are dimmed with a moon icon; clicking one wakes it. Pinned/audible exemptions. |
| Lazy restore | ✅ | Only the visible tab is created after a restart (benchmark: 20 restored tabs → 1 live). |
| Limit on background processes | ✅ | `--renderer-process-limit` setting (applies after restart). |
| Memory meter with per-tab RAM + "Free memory" | 🟡 | "Free memory" works. The meter lists processes and tabs, but CEF's TaskManager reports **0 bytes** per task (a binding bug was worked around; memory refresh still isn't enabled). Needs a fallback that reads OS process memory. |
| Minimal main-process work | ✅ | No polling except a 15 s housekeeping tick and a 1 s timer refresh while a Pomodoro runs. |
| Benchmarks 1/5/20 tabs | ✅ | [BENCHMARKS.md](BENCHMARKS.md) (Linux numbers). |

## Setup wizard & accounts

| | Status | Notes |
|---|---|---|
| First-run wizard: theme, layout, ADHD features | ✅ | Live theme switching while you choose. |
| Import bookmarks/history from Chrome & Edge profile folders | ✅ | Tested on a synthetic Chromium profile, plus a real Brave profile detected on the dev machine. |
| Import passwords from Chrome & Edge | 🟡 | Works for Edge and Chrome < 127 on Windows (DPAPI `v10`) and Linux `v10`. **Chrome ≥ 127 on Windows uses app-bound encryption (`v20`) that only Chrome can unlock**, and Linux keyring (`v11`) / macOS Keychain aren't read. In those cases the wizard explains how to export a CSV and import that instead. |
| HTML bookmarks / CSV passwords fallback | ✅ | Netscape HTML; CSV from Chrome, Edge, Firefox, Bitwarden and 1Password exports. |
| "Sign in to Google" / "Sign in to Microsoft" | 🟡 | Opens the real sign-in pages in a tab. Emerald sends a standard Chromium user agent and client hints (no embedded-browser markers, no automation flags). **Not verified end to end**: it needs a real account, and Google's risk checks can't be tested from here. |
| Chrome Sync / Edge sync | ❌ | Not offered to third-party browsers. Alternatives: one-time import, and a cross-browser password manager extension. |

## Extensions

| | Status | Notes |
|---|---|---|
| "Add to Emerald" on Chrome Web Store | ✅ | Relabels the button; Chrome's own installer runs. Real store IDs and auto-updates. Verified with uBlock Origin Lite, Dark Reader, Bitwarden. |
| "Add to Emerald" on Edge Add-ons | ✅ | Downloads the CRX, **verifies the CRX3 RSA signature**, shows a permission prompt, then installs under the same ID as Edge. Verified with React Developer Tools. No auto-update for Edge-sourced extensions yet (🟡). |
| Extensions manager: enable/disable/remove, pin to toolbar | ✅ | Via `chrome.developerPrivate` / `chrome.management` on a hidden `chrome://extensions` page. |
| Popups | 🟡 | Open in a panel under the toolbar icon (Bitwarden and uBOL verified). Popups that query "the current tab" see the popup itself (uBOL's per-site panel says "not a website"), because Chromium's tab strip isn't used. |
| Manifest V3 | ✅ | |
| Manifest V2 | ❌ | Chromium 156 has removed MV2. |
| uBlock Origin Lite | ✅ | Blocks ads/trackers (tested with the built-in blocker turned off). |
| Bitwarden | ✅ | Service worker runs; popup works; welcome page opens as a tab. Vault login not tested (needs an account). |
| Dark Reader | ✅ | Restyles pages (verified on Wikipedia). |
| React DevTools | 🟡 | Installed from Edge Add-ons; its page hook is injected. The DevTools "Components" panel was not inspected visually. |

## Customization

| | Status | Notes |
|---|---|---|
| Themes as JSON | ✅ | `themes/*.json`; user themes in the profile's `themes/` folder. |
| Live editor: colors, fonts, radius, density, tab layout, start page | ✅ | Live preview pane; save & apply. |
| Custom CSS for the browser UI | ✅ | Settings → Appearance (plus per-theme CSS). |
| Import/export theme packs | ✅ | One JSON file. Imported themes are validated and their CSS can't load remote resources. |
| 5+ built-in themes, emerald default light + dark | ✅ | 7: Emerald Dark, Emerald Light, Midnight (OLED), Paper (calm), Lagoon, Dusk Lavender, High Contrast. |

## Tabs & organization

| | Status | Notes |
|---|---|---|
| Folders, groups | ✅ | Collapsible, colored, rename, dissolve or delete. |
| Workspaces / spaces | ✅ | Emoji + color; move tabs between spaces (drag onto a space chip). |
| Pinned tabs | ✅ | |
| Split view | ✅ | Two tabs side by side (`Ctrl Shift E`, Ctrl-click, or the context menu). |
| Drag-and-drop | ✅ | Reorder, into or out of folders, to pinned, to other spaces. |
| Fuzzy command bar (`Ctrl K`) | ✅ | |
| Customizable start page with draggable widgets | ✅ | 7 widget types on a 12-column grid. |

## ADHD-friendly mode

| | Status | Notes |
|---|---|---|
| Focus mode (only the current tab) | ✅ | |
| Optional tab limit with gentle nudges | ✅ | |
| Site blocker + distraction timer + scheduled focus sessions | ✅ | Blocks typed and in-page navigations; calm block page with "allow 5 minutes"; weekly schedules; optional "still what you meant to do?" nudge after N minutes. |
| Pomodoro / timer | ✅ | Sidebar chip, start-page widget, phase notices. |
| Quick-capture notes | ✅ | `Ctrl Shift N`, start-page widget, "save selection to notes" in the context menu. |
| Park this tab for later + reminders | ✅ | Reminder presets; reminder notice with "Open it". |
| Reader mode | ✅ | Own extraction heuristic; font size, width, serif/sans/dyslexic controls. |
| Distraction-free video mode | ✅ | YouTube and any `<video>`. |
| Calm low-motion UI, reduced-animation toggle, big targets, clear hierarchy | ✅ | |
| Dyslexia-friendly font | ✅ | OpenDyslexic bundled (OFL), applied to the UI and reader mode. |
| No pop-up spam, quiet by default | ✅ | Notification permission prompts are declined silently in quiet mode; no toasts except ones you asked for. |

## Privacy & basics

| | Status | Notes |
|---|---|---|
| Built-in lightweight ad/tracker blocking | ✅ | About 150 curated tracker/ad domains, matched by suffix. Deliberately small: install uBOL for full lists. |
| HTTPS-First | 🟡 | Chromium's `https_only_mode_enabled` pref is set; whether CEF honours it in every case wasn't separately verified. |
| Per-site permissions | ✅ | Emerald's own prompt bar; remembered decisions; reset in Settings. |
| Downloads manager | ✅ | Saves to the downloads folder without dialogs; pause/resume/cancel/open/show. |
| History | ✅ | Own SQLite, searchable from the command bar. |
| Find in page | ✅ | |
| DevTools | ✅ | `F12` (opens in its own window). |
| Print | 🟡 | Calls Chromium's print; the print preview UI wasn't exercised in testing. |
| PDF viewer | 🟡 | Chromium's built-in viewer is part of Chrome style; not separately tested. |
| Password autofill, encrypted locally | ✅ | Offers to save on submit; fill is one click in Emerald's bar, never automatic. AES-256-GCM vault (key DPAPI-protected on Windows; a 0600 key file on Linux/macOS, which is weaker; see vault.rs). Verified on a local test page. |

## Packaging

| | Status | Notes |
|---|---|---|
| Windows installer (Inno Setup, per-user) + portable zip | 🟡 | Built by CI (installer 131 MB, zip 189 MB) and tests pass on Windows; **not yet launched on a real Windows PC**. |
| GitHub Actions release workflow | ✅ | Builds Linux + Windows; publishes a release on `v*` tags. The macOS job is paused. |
| Linux tarball | ✅ | Built locally and in CI; the extracted package runs with no dev setup. |
| Windows sandbox | ⬜ | Builds currently run with `no_sandbox` (the cef-rs Windows sandbox needs its `bootstrap.exe` layout). Top priority before any public release. |
| Linux sandbox | ⬜ | Same: needs the SUID `chrome-sandbox` helper set up by a package. |
