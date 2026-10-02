# NocFree Companion

A read-only macOS prototype using GPUI and `iamnbutler/gpuikit`. It starts with the left half, shows one physical step at a time, and saves a private firmware copy automatically after fresh connection and metadata checks. Once normal startup returns, it guides the right half. There are no firmware-write, reset, erase or serial-port commands.

This version recognizes the ANSI factory left and this project's Mac USB diagnostic on the right. Receiver support, factory-right onboarding and Windows/Linux discovery are pending. The prescribed physical role and USB connection are correlated; identity alone does not prove the role, independent recovery, or restore compatibility. The app asks for the dongle to remain disconnected because it shares the factory left's identity.

## Run

For a developer build on macOS:

```sh
./desktop/build-macos.sh
open 'dist/NocFree Companion.app'
```

Rust 1.98.1 is pinned separately from the firmware. The lockfile aligns the unofficial GPUI distribution crates at 1.14.2 with gpuikit 0.9.0; use locked builds. Runtime Metal shader compilation avoids requiring the optional Xcode Metal compiler. The generated bundle is locally signed for development, not a notarized newcomer release. Firmware dependencies are unchanged.

## Recovery archives

After the owner enters recovery using the displayed shortcut, the app automatically reads the mounted drive and writes only to `~/Library/Application Support/NocFree Companion/readback-<timestamp>/`. Files include CURRENT.UF2, bootloader metadata and a SHA-256/coverage manifest. Directories are private (0700), files are private (0600), and each file is flushed and finalized atomically. Failed saves stop the flow; incomplete files may remain for inspection. Retry preserves an already saved copy and its return guide, resetting observation evidence and countdowns. An uncertain save is never automatically retried.

The archive parser validates the complete exposed readback container, not firmware-installation policy. It does not replace the canonical [image guard](../scripts/image_guard.py) or [migration guard](../scripts/migration_guard.py). The readable span excludes MBR, filesystem, bootloader and UICR. An archive is neither a full-chip backup nor a proven restore image. Factory bytes and local identifiers never belong in Git.

The manifest records the archive; it does not resume a prior workflow. Restart requires fresh normal-mode identification. USB disappearance/reappearance changes the screen automatically, and return countdowns begin only after observed absence. The right-side battery-start countdown requires one acknowledgment that its switch was turned ON while USB remains unplugged. Early reconnection or observation errors invalidate the wait. The host cannot observe switch position or battery power. Technical policy stays in these developer notes and local manifests, outside the main flow.

## Verification

```sh
cd desktop
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

Replay tests cover stale/error observations, late mounts, ambiguous bootloaders, changed connections, removal during backup, restart, archive coverage and private saved files. These are host checks. Actual app launch, live observations, physical recovery/readback and device acceptance are separate evidence.

Observed locally on 2026-10-01: the app built and launched with a compact single-task layout. Its automatic left-first flow removes role, Details and Save choices. Eleven replay/archive/return-guide tests, strict Clippy checks and formatting passed.

The owner then completed the guided left and right copy flow and reported that it was smooth. Independent inspection confirmed that both private archives contain 1,728 valid UF2 blocks covering 0x1000–0x6d000, their hashes match their manifests, and their bytes match the previously verified left factory image and right Mac diagnostic respectively. Directory and file permissions were 0700 and 0600. A subsequent read-only USB inventory observed both normal-mode identities, no NocFree bootloader and no mounted recovery readback. This validates the copy flow on this Mac and these two firmware states. It does not validate firmware installation, restore compatibility, independent cold-start recovery or other operating systems; earlier guarded firmware trials remain separate evidence.

The development bundle currently relies on debug asset loading from the local Cargo source directory. It is suitable on this Mac; a portable newcomer download needs embedded assets or a release build, signing/notarization and its own installation tests.

The [installer design](../docs/research/guided-installer-app.md) describes the later guarded writer and cross-OS release work. This prototype has no Install or Restore action.
