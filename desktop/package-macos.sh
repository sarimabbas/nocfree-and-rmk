#!/bin/sh
set -eu
case "${1:-}" in
  -h|--help)
    echo 'Build an unsigned Apple Silicon release app and ZIP for signing.'
    echo 'Usage: desktop/package-macos.sh [--help | --version]'
    echo 'Requires the reviewed dist/companion-firmware package; never builds firmware.'
    exit 0 ;;
  -v|--version) exec "$(dirname "$0")/build-macos.sh" --version ;;
  '') ;;
  *) echo 'Unknown option. Use --help.' >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { echo 'Unexpected arguments. Use --help.' >&2; exit 2; }
[ "$(uname -m)" = arm64 ] || { echo 'Release packaging requires Apple Silicon.' >&2; exit 1; }
cd "$(dirname "$0")"
REGISTER_APP=0 ./build-macos.sh --release
version=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "nocfree-companion"))')
archive="../dist/nocfree-rmk-companion-${version}-macos-arm64-unsigned.zip"
/usr/bin/ditto -c -k --keepParent '../dist/NocFree RMK Companion.app' "$archive"
(cd ../dist && shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256")
echo "Unsigned archive ready: $archive"
