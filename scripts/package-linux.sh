#!/usr/bin/env bash
# Bundle a release build with the CEF runtime into dist/emerald-<version>-linux-x64.tar.gz
#   CEF_PATH=~/.local/share/cef scripts/package-linux.sh 0.1.0
set -euo pipefail
version=${1:-0.1.0}
here=$(cd "$(dirname "$0")/.." && pwd)
: "${CEF_PATH:?set CEF_PATH to an exported CEF directory}"
out="$here/dist/emerald"
rm -rf "$out" && mkdir -p "$out"
cp "$here/target/release/emerald" "$out/"
# CEF runtime: libraries, resources, locales. Headers/CMake/wrapper sources are build-only.
for f in "$CEF_PATH"/*; do
  case "$(basename "$f")" in include|cmake|libcef_dll|CMakeLists.txt|archive.json|*.a) continue ;; esac
  cp -r "$f" "$out/"
done
cp "$here/packaging/linux/emerald.desktop" "$out/"
cp "$here/ui/img/emerald-256.png" "$out/emerald.png"
cp "$here/README.md" "$here/LICENSE-MIT" "$here/LICENSE-APACHE" "$out/" 2>/dev/null || true
cp "$here/ui/fonts/OpenDyslexic-OFL.txt" "$out/"
cat > "$out/install-desktop-entry.sh" <<'IN'
#!/bin/sh
# Adds Emerald to your application menu (per user).
d=$(cd "$(dirname "$0")" && pwd)
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
sed "s|^Exec=emerald|Exec=\"$d/emerald\"|" "$d/emerald.desktop" > ~/.local/share/applications/emerald.desktop
cp "$d/emerald.png" ~/.local/share/icons/hicolor/256x256/apps/emerald.png
echo "Emerald added to your applications menu."
IN
chmod +x "$out/install-desktop-entry.sh"
strip "$out/libcef.so" 2>/dev/null || true
tar -C "$here/dist" -czf "$here/dist/emerald-$version-linux-x64.tar.gz" emerald
du -sh "$out" "$here/dist/emerald-$version-linux-x64.tar.gz"
