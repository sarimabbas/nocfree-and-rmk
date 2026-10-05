# macOS release preparation

The release workflow builds Apple Silicon only, with a macOS 13.0 deployment target and matching `LSMinimumSystemVersion`. This is a compatibility policy, not proof of testing on macOS 13. Local acceptance is on macOS 27; older supported systems still need an app launch, permission, backup, install, pairing and typing check before broad distribution.

`desktop/build-macos.sh` keeps the existing debug/ad-hoc build as its default. `--release` uses the optimized Cargo profile; Cargo supplies both the app version and `--version` output. `BUILD_NUMBER` supplies the numeric bundle build version. `desktop/package-macos.sh` builds a release app and an explicitly **unsigned** ZIP and SHA256, without registering the release app with Launch Services. Developer ID signing and notarization happen separately in CI.

## Reviewed firmware input

CI must never regenerate firmware from current source or manufacture hardware acceptance. On the evidence owner's Mac:

```sh
python3 scripts/package_companion_release.py --check
python3 scripts/package_companion_release.py
(cd dist/companion-firmware && zip ../companion-firmware.zip manifest.json left.uf2 left.bin right.uf2 right.bin receiver.uf2 receiver.bin)
shasum -a 256 dist/companion-firmware.zip
```

Review the output package and checksum. Store this ZIP as the `companion-firmware.zip` asset of a **same-repository staging release**; uploading it is a separate maintainer action. Do not upload the private evidence, factory originals, recovery executables, or device identifiers. The workflow requires the staging release tag and the reviewed ZIP SHA256. Its extractor accepts exactly seven root files with bounded sizes, rejects traversal/duplicates/extra files, and refuses an existing destination. The app's `--check-firmware` then verifies the pinned manifest and the exact role-specific UF2/BIN image guards. Merely passing those structural checks is not hardware validation.

The current fixed firmware remains `0.1.0-local.2`; its evidence limitations are in [companion-firmware-release.md](research/companion-firmware-release.md). Updating it requires a separately reviewed package and updating the Rust manifest pin, not changing a workflow input alone.

## Signing and draft release

The workflow runs only from `main`, rejects an existing version tag, and requires the version to match `desktop/Cargo.toml`. Configure a protected GitHub `release` environment restricted to `main` with:

- `MACOS_CERTIFICATE_P12`: base64 Developer ID Application certificate/private key export.
- `MACOS_CERTIFICATE_PASSWORD`.
- `APPLE_API_PRIVATE_KEY_P8`: base64 App Store Connect API private key.
- `APPLE_API_KEY_ID` and `APPLE_API_ISSUER_ID`.

It follows [Vinny's release workflow](https://github.com/sarimabbas/vinny/blob/main/.github/workflows/release.yml): build without signing credentials, import into an ephemeral signing keychain only after environment approval, sign with hardened runtime/timestamp, submit to Apple, staple, verify signatures/Gatekeeper, and produce ZIP plus SHA256. Cleanup runs even on failure. The final GitHub release is a **draft**; no automatic publication or Homebrew mutation occurs.

Start **Draft macOS release** only after the staging asset and environment secrets exist. Review the draft artifact on a fresh Mac, including removable-volume permission refusal/retry and factory/RMK journeys. Publish manually only after acceptance. Do not replace published archives or rewrite release tags.

## Current blockers

No signing/notary secrets were configured in the repository when this workflow was prepared; environment secrets have not been provisioned by this change. The reviewed firmware staging asset also needs preparation/upload. Signing and notarization have not been exercised here.

The bundle includes the project MIT license and `desktop/THIRD_PARTY_NOTICES.md`, generated from the locked macOS dependency graph by `scripts/companion_notices.py`. The committed file is the desktop license inventory; the builder generates available full license/notice texts into the app bundle and explicitly marks crates whose published packages omit license texts. Bundled firmware uses separate pinned RMK/Embassy dependency graphs, including different RIGHT and LEFT/dongle revisions; those notices require a separate inventory. Resolve those marked upstream notices and audit embedded assets/firmware licensing as a mandatory prepublication gate; this inventory is not a completed legal audit.
