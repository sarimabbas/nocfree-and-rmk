#!/bin/sh
set -eu

case "${1:-}" in
  -h|--help)
    echo 'Build the local Nocfree RMK Companion macOS app.'
    echo 'Usage: desktop/build-macos.sh [--help | --version]'
    echo 'Requires Rust, macOS and Apple command-line tools. Creates dist/NocFree Companion.app.'
    exit 0 ;;
  -v|--version) echo 'Nocfree RMK Companion 0.1.0'; exit 0 ;;
  '') ;;
  *) echo 'Unknown option. Use --help.' >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { echo 'Unexpected arguments. Use --help.' >&2; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo 'This prototype supports macOS only.' >&2; exit 1; }
cd "$(dirname "$0")"
echo 'Building Nocfree RMK Companion…'
cargo build --locked
bundle='../dist/NocFree Companion.app'
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
icon_work=$(mktemp -d "${TMPDIR:-/tmp}/nocfree-icon.XXXXXX")
trap 'rm -rf "$icon_work"' EXIT HUP INT TERM
/usr/bin/qlmanage -t -s 1024 -o "$icon_work" assets/companion-icon.svg >/dev/null
/usr/bin/swift render-icon.swift "$icon_work/companion-icon.svg.png" "$icon_work/icon.png"
mkdir "$icon_work/Companion.iconset"
for size in 16 32 128 256 512; do
  /usr/bin/sips -z "$size" "$size" "$icon_work/icon.png" --out "$icon_work/Companion.iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  /usr/bin/sips -z "$double" "$double" "$icon_work/icon.png" --out "$icon_work/Companion.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
/usr/bin/iconutil -c icns "$icon_work/Companion.iconset" -o "$bundle/Contents/Resources/Companion.icns"
if [ -f ../dist/companion-firmware/manifest.json ]; then
    mkdir -p "$bundle/Contents/Resources/Firmware"
    cp ../dist/companion-firmware/* "$bundle/Contents/Resources/Firmware/"
fi
cp target/debug/nocfree-companion "$bundle/Contents/MacOS/nocfree-companion.new"
mv "$bundle/Contents/MacOS/nocfree-companion.new" "$bundle/Contents/MacOS/nocfree-companion"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>nocfree-companion</string>
<key>CFBundleIdentifier</key><string>io.github.sarimabbas.nocfree-companion</string>
<key>CFBundleName</key><string>Nocfree RMK Companion</string>
<key>CFBundleDisplayName</key><string>Nocfree RMK Companion</string>
<key>CFBundleIconFile</key><string>Companion.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
/usr/bin/plutil -lint "$bundle/Contents/Info.plist"
/usr/bin/codesign --force --sign - "$bundle"
# Refresh this bundle only so Launch Services sees the new name and Dock icon.
/usr/bin/touch "$bundle"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$bundle"
echo 'Ready: dist/NocFree Companion.app (local development signature)'
