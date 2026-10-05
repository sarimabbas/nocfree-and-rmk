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

It follows [Vinny's release workflow](https://github.com/sarimabbas/vinny/blob/main/.github/workflows/release.yml): build without signing credentials, import into an ephemeral signing keychain in the main-restricted release job, sign with hardened runtime/timestamp, submit to Apple, staple, verify signatures/Gatekeeper, and produce ZIP plus SHA256. Cleanup runs even on failure. The final GitHub release is a **draft**; no automatic publication or Homebrew mutation occurs.

Start **Draft macOS release** only after the staging asset and environment secrets exist. Review the draft artifact on a fresh Mac, including removable-volume permission refusal/retry and factory/RMK journeys. Publish manually only after acceptance. Do not replace published archives or rewrite release tags.

## Current blockers

Read-only audit on 2026-10-04 found zero valid local code-signing identities (`security find-identity -v -p codesigning`), zero NocFree repository secrets. The `release` environment has now been created with custom deployment policies permitting only the `main` branch; API readback confirmed that exact single branch policy and zero environment secrets. Required-reviewer approval is not configured; this setup added no reviewers or other principals and changed no repository-wide protections. Vinny's separate `release` environment contains the five required Apple certificate/notary secret names; their values cannot be read back from GitHub or automatically reused by this repository. No credentials were exported and no keychain or secret settings were changed. Signing/notarization rehearsal is therefore blocked on provisioning a valid Developer ID identity or this repository's protected release secrets.

The reviewed firmware ZIP is prepared locally at `dist/companion-firmware.zip` with SHA256 `64508bd93f437cf67e7ffe5ab385be5203fd30fdc75401e9984223275e96a3cf` (1,323,451 bytes), alongside its `.sha256` file. The private-evidence packager and Companion's pinned package guard both passed. It has not been uploaded. Its hash binds these exact ZIP bytes; recreating it with another ZIP implementation can change the ZIP hash without changing the firmware.

To unblock the workflow, a maintainer must supply the five secrets in the main-restricted `release` environment from their encrypted source backups and upload the reviewed ZIP to a same-repository staging release. Then start **Draft macOS release** from `main` with the Cargo version, staging tag and reviewed ZIP hash. Secret provisioning and upload remain separate from this preparation; no tags, uploads or releases were created here.

The bundle includes the project MIT license and full `THIRD_PARTY_NOTICES.md`, generated offline by `scripts/companion_notices.py`. The reviewed source inventory now covers the desktop graph and both exact bundled firmware source graphs, including the different RIGHT RMK revision. Hash-named upstream notice texts are cached under `docs/notices/texts/`; source revisions and package checksums are recorded in `inventory.json`. The committed `desktop/THIRD_PARTY_NOTICES.md` is only the catalog; the builder includes the full text appendix.

Twelve source-text gaps remain explicitly listed in the catalog and [notice capture documentation](notices/README.md). Draft builds remain allowed. Before public publication, run the strict check on a newly generated full notice file:

```sh
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md --strict --check
```

The strict command currently fails on those twelve gaps; resolve them from immutable upstream sources and review the applicable desktop, asset and firmware redistribution terms before publishing. Merely including SPDX declarations or passing image guards does not complete the notice audit.

## Maintaining the notices snapshot

The generator deliberately fails after any desktop Cargo.lock/Cargo.toml change or a bundled firmware source/RMK pin change. Do not bypass this guard during a version bump. Follow [the capture procedure](notices/README.md) in a temporary source checkout: resolve locked metadata for Apple Silicon, capture the selected dependency closure and package archive/VCS provenance, collect source notices from exact immutable revisions, and update the reviewed inventory and cached text hashes. For changed bundled firmware pins, repeat against each exact pinned source commit and `thumbv7em-none-eabihf` graph; the renderer does not need those historical checkouts in CI.

An app-version-only Cargo.toml update can keep the existing dependency texts when a fresh locked metadata comparison confirms the dependency graph is unchanged. Review that comparison, then update the recorded desktop manifest hash; dependency changes require recapture rather than only replacing the hash. Regenerate the catalog with `--inventory`, run `--inventory --check`, the offline notice tests, and the full strict publication check. Commit the inventory, text changes and regenerated catalog together. No private evidence, build products or machine-specific paths belong in this snapshot.
