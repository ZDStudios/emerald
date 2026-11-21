#!/usr/bin/env bash
# Build Emerald.app (+ zip) from a release build and an exported CEF directory. macOS only.
#   CEF_PATH=~/.local/share/cef scripts/package-macos.sh 0.1.0
#
# Layout Chromium expects:
#   Emerald.app/Contents/MacOS/emerald
#   Emerald.app/Contents/Frameworks/Chromium Embedded Framework.framework
#   Emerald.app/Contents/Frameworks/emerald Helper{, (GPU), (Renderer), (Plugin), (Alerts)}.app
# The helpers are copies of the same binary (main.rs detects " Helper" in its name).
set -euo pipefail
version=${1:-0.1.0}
here=$(cd "$(dirname "$0")/.." && pwd)
: "${CEF_PATH:?set CEF_PATH to an exported CEF directory}"
framework=$(find "$CEF_PATH" -maxdepth 3 -type d -name "Chromium Embedded Framework.framework" | head -1)
[ -n "$framework" ] || { echo "Chromium Embedded Framework.framework not found under $CEF_PATH"; ls -la "$CEF_PATH"; exit 1; }
bin="$here/target/release/emerald"
out="$here/dist/macos"
app="$out/Emerald.app"
rm -rf "$out" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources"

plist() { # $1=path $2=executable $3=bundle id $4=name $5=extra keys
  cat > "$1" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>$2</string>
  <key>CFBundleIdentifier</key><string>$3</string>
  <key>CFBundleName</key><string>$4</string>
  <key>CFBundleDisplayName</key><string>$4</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  $5
</dict></plist>
EOF
}

cp "$bin" "$app/Contents/MacOS/emerald"
plist "$app/Contents/Info.plist" emerald org.emerald.browser Emerald '<key>CFBundleIconFile</key><string>emerald</string>
  <key>NSCameraUsageDescription</key><string>A website wants to use the camera.</string>
  <key>NSMicrophoneUsageDescription</key><string>A website wants to use the microphone.</string>
  <key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>Web site URL</string><key>CFBundleURLSchemes</key><array><string>http</string><string>https</string></array></dict></array>'
cp -R "$framework" "$app/Contents/Frameworks/"

for suffix in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
  name="emerald Helper$suffix"
  h="$app/Contents/Frameworks/$name.app"
  mkdir -p "$h/Contents/MacOS"
  cp "$bin" "$h/Contents/MacOS/$name"
  id_suffix=$(echo "$suffix" | tr -d ' ()' | tr '[:upper:]' '[:lower:]')
  plist "$h/Contents/Info.plist" "$name" "org.emerald.browser.helper${id_suffix:+.$id_suffix}" "$name" '<key>LSUIElement</key><string>1</string>'
done

# Icon
iconset="$out/emerald.iconset"
mkdir -p "$iconset"
for s in 16 32 64 128 256 512; do
  sips -z $s $s "$here/ui/img/emerald-256.png" --out "$iconset/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); sips -z $d $d "$here/ui/img/emerald-256.png" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/emerald.icns"
rm -rf "$iconset"

cp "$here/README.md" "$here/ui/fonts/OpenDyslexic-OFL.txt" "$out/"
# Ad-hoc signature: required for arm64 binaries; not notarized (see README for first launch).
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app" && echo "signature ok (ad-hoc)"
arch=$(uname -m)
(cd "$out" && ditto -c -k --sequesterRsrc --keepParent Emerald.app "$here/dist/Emerald-$version-macos-$arch.zip")
du -sh "$app" "$here/dist/Emerald-$version-macos-$arch.zip"
