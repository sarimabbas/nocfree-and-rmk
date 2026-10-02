#!/bin/sh
set -eu

case "${1:-}" in
  -h|--help)
    echo 'Build the local read-only NocFree Companion macOS app.'
    echo 'Usage: desktop/build-macos.sh [--help | --version]'
    echo 'Requires Rust and macOS. Creates dist/NocFree Companion.app.'
    exit 0 ;;
  -v|--version) echo 'NocFree Companion 0.1.0'; exit 0 ;;
  '') ;;
  *) echo 'Unknown option. Use --help.' >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { echo 'Unexpected arguments. Use --help.' >&2; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo 'This prototype supports macOS only.' >&2; exit 1; }
cd "$(dirname "$0")"
echo 'Building the read-only companion…'
cargo build --locked
bundle='../dist/NocFree Companion.app'
mkdir -p "$bundle/Contents/MacOS"
cp target/debug/nocfree-companion "$bundle/Contents/MacOS/nocfree-companion.new"
mv "$bundle/Contents/MacOS/nocfree-companion.new" "$bundle/Contents/MacOS/nocfree-companion"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>nocfree-companion</string>
<key>CFBundleIdentifier</key><string>io.github.sarimabbas.nocfree-companion</string>
<key>CFBundleName</key><string>NocFree Companion</string>
<key>CFBundleDisplayName</key><string>NocFree Companion</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
/usr/bin/codesign --force --sign - "$bundle"
echo 'Ready: dist/NocFree Companion.app (local development signature)'
