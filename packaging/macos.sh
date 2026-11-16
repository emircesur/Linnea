#!/usr/bin/env bash
# Linnea.app and a disk image from a release build:  bash packaging/macos.sh <version or tag> <arm64|x64>
# Run from the repository root after `cargo build --release -p linnea-cli -p linnea-app` and the UI build.
set -euo pipefail
ver="${1:-}"; arch="${2:-$(uname -m)}"
ver="${ver#v}"
[[ "$ver" =~ ^[0-9] ]] || ver="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
target="${CARGO_TARGET_DIR:-target}/release"

app="dist/Linnea.app"
rm -rf "$app" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/ui"
cp "$target/linnea-app" "$target/linnea" "$app/Contents/MacOS/"
cp -R library skill README.md "$app/Contents/Resources/"
cp -R ui/dist "$app/Contents/Resources/ui/dist"

# the icon: the 256 px PNG scaled into an iconset
set +e
icons="$(mktemp -d)/linnea.iconset"; mkdir -p "$icons"
for s in 16 32 128 256; do
  sips -z $s $s assets/icon.png --out "$icons/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); [ $d -le 256 ] && sips -z $d $d assets/icon.png --out "$icons/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/linnea.icns" || echo "no icon made"
set -e

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Linnea</string>
  <key>CFBundleDisplayName</key><string>Linnea</string>
  <key>CFBundleIdentifier</key><string>io.github.emircesur.linnea</string>
  <key>CFBundleVersion</key><string>$ver</string>
  <key>CFBundleShortVersionString</key><string>$ver</string>
  <key>CFBundleExecutable</key><string>linnea-app</string>
  <key>CFBundleIconFile</key><string>linnea</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# an ad-hoc signature, so Apple silicon runs it (it is not notarized: right-click > Open the first time)
codesign --force --deep --sign - "$app"

out="dist/Linnea-$ver-macos-$arch.dmg"
stage="$(mktemp -d)"
cp -R "$app" "$stage/" && ln -s /Applications "$stage/Applications"
hdiutil create -volname "Linnea" -srcfolder "$stage" -ov -format UDZO "$out"
echo "made $out"
