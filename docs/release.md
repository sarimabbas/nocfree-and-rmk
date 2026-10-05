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

The reviewed bundled firmware is `0.1.1`, release `nocfree-a717808eec6f8dfe`. All three application images have exact installed readback verification and current-image Companion recovery observations. Source is `fb45eecef29f2a2883f44cd6e28843798238d0a7`, with RMK `89fead1de856132911ec685313fa88cda0652bac`. See [acceptance scope](research/main-firmware-update.md).

## Signing and draft release

The workflow runs only from `main`, rejects an existing version tag, and requires the version to match `desktop/Cargo.toml`. Configure a protected GitHub `release` environment restricted to `main` with:

- `MACOS_CERTIFICATE_P12`: base64 Developer ID Application certificate/private key export.
- `MACOS_CERTIFICATE_PASSWORD`.
- `APPLE_API_PRIVATE_KEY_P8`: base64 App Store Connect API private key.
- `APPLE_API_KEY_ID` and `APPLE_API_ISSUER_ID`.

It follows [Vinny's release workflow](https://github.com/sarimabbas/vinny/blob/main/.github/workflows/release.yml): build without signing credentials, import into an ephemeral signing keychain in the main-restricted release job, sign with hardened runtime/timestamp, submit to Apple, staple, verify signatures/Gatekeeper, and produce ZIP plus SHA256. Cleanup runs even on failure. The final GitHub release is a **draft**; no automatic publication or Homebrew mutation occurs.

Start **Draft macOS release** only after the staging asset and environment secrets exist. Review the draft artifact on a fresh Mac, including removable-volume permission refusal/retry and factory/RMK journeys. Publish manually only after acceptance. Do not replace published archives or rewrite release tags.

## Public preview status

The first release is an Apple Silicon public preview. The release notes distinguish owner observations from measured latency/power, document the upstream split-BLE held-key disconnect limitation, provisional battery calibration, ANSI acceptance scope and older/clean-Mac testing limits. Those limits do not claim a separately completed compatibility or endurance test.

The reviewed firmware ZIP is `dist/companion-firmware.zip`, SHA256 `5b921799000a861afb74c4e1f3fa0ca7423002299f34741d16807b653f20e09c` (1,311,135 bytes). Its exact seven-file extraction and Companion manifest/image guards passed. Only these public application images and manifest belong in staging; private evidence and factory originals stay local.

Signing credentials are provisioned from the owner’s Bitwarden vault into the repository’s main-restricted `release` environment. The workflow validates the offline publication notices, builds against the pinned firmware, signs and notarizes, staples and checks Gatekeeper, then creates a draft prerelease. A maintainer reviews the resulting archive before publishing it. No published archive or release tag may be replaced.

The bundle includes the project MIT license, NOTICE.md and full third-party notice appendix. Generate and check it before publication:

```sh
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md --strict --check
```

The source acquisition and upstream-omission policy is documented in [notice capture documentation](notices/README.md). A successful check verifies the recorded source/text inventory; it does not manufacture missing upstream copyright statements.

## Maintaining the notices snapshot

The generator deliberately fails after any desktop Cargo.lock/Cargo.toml change or a bundled firmware source/RMK pin change. Do not bypass this guard during a version bump. Follow [the capture procedure](notices/README.md) in a temporary source checkout: resolve locked metadata for Apple Silicon, capture the selected dependency closure and package archive/VCS provenance, collect source notices from exact immutable revisions, and update the reviewed inventory and cached text hashes. For changed bundled firmware pins, repeat against each exact pinned source commit and `thumbv7em-none-eabihf` graph; the renderer does not need those historical checkouts in CI.

An app-version-only Cargo.toml update can keep the existing dependency texts when a fresh locked metadata comparison confirms the dependency graph is unchanged. Review that comparison, then update the recorded desktop manifest hash; dependency changes require recapture rather than only replacing the hash. Regenerate the catalog with `--inventory`, run `--inventory --check`, the offline notice tests, and the full strict publication check. Commit the inventory, text changes and regenerated catalog together. No private evidence, build products or machine-specific paths belong in this snapshot.
