# Running Emerald

Builds come from GitHub Actions (**Actions → Build & release → latest run → Artifacts**) or,
for tagged versions, the **Releases** page. Linux can also be built locally
(`scripts/package-linux.sh`).

> These are unsigned development builds. Windows and macOS will warn about an unknown
> developer the first time you run them. Instructions for each OS are below.

## Linux (x64)

```bash
tar -xzf emerald-0.1.0-linux-x64.tar.gz
./emerald/emerald
```

Optional: run `./emerald/install-desktop-entry.sh` to add Emerald to your applications menu.

* Works on X11 and on Wayland desktops (it runs through XWayland automatically).
* Data is stored in `~/.local/share/emerald`.
* If it doesn't start, run it from a terminal and look at the output. Missing system libraries
  (`libnss3`, `libgbm1`, `libasound2`) are the usual cause, and the same packages Chrome needs fix it.

## Windows (x64, Windows 10 1809 or later)

From the `emerald-windows-x64` artifact, pick one:

* **Installer:** run `Emerald-…-setup-x64.exe`. It installs for your user only (no admin
  rights) and adds a Start-menu entry. When SmartScreen says "Windows protected your PC",
  choose **More info → Run anyway**, because the build isn't code-signed.
* **Portable:** unzip `Emerald-…-portable-x64.zip` anywhere and run `Emerald\emerald.exe`.

Data is stored in `%LOCALAPPDATA%\Emerald`. To make Emerald your default browser, tick "Make
Emerald available as a default-browser choice" in the installer, then pick it in
**Settings → Apps → Default apps**.

## macOS (paused)

The macOS port is written but its CI build is paused, so there is no macOS download yet.
The steps below apply once it's re-enabled.

### macOS (Apple Silicon, macOS 11 or later)

From the `emerald-macos-arm64` artifact:

1. Unzip `Emerald-…-macos-arm64.zip` and drag **Emerald.app** to Applications.
2. The app is ad-hoc signed but not notarized, so on first launch **right-click (or
   Control-click) Emerald.app → Open → Open**. If macOS still refuses ("is damaged"), run this once:
   ```bash
   xattr -dr com.apple.quarantine /Applications/Emerald.app
   ```

Data is stored in `~/Library/Application Support/Emerald`. Intel Macs aren't built yet.

## Useful options (all platforms)

| Option | |
|---|---|
| `--profile=<folder>` | Use a separate profile folder (portable mode, testing) |
| `--native-frame` | Use the operating system's title bar instead of Emerald's |
| `--maximized` | Start maximized |
| `<url>` | Open a URL (also how links from other apps arrive when Emerald is the default browser) |
