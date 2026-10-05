# Dependency notice sources

`inventory.json` records the resolved desktop dependencies and two pinned firmware source graphs. `texts/` contains unmodified UTF-8 upstream notice files named by their SHA-256. `desktop/THIRD_PARTY_NOTICES.md` is the checked-in catalog; the bundle generator reproduces complete text in a deduplicated appendix. Newline conventions are normalized only in the generated Markdown, not in the preserved source files.

Generate or verify the notices without network access, Cargo, Git history, private build evidence, or connected hardware:

```sh
python3 scripts/companion_notices.py --inventory
python3 scripts/companion_notices.py --inventory --check
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md --strict --check
python3 -m unittest discover -s scripts -p test_companion_notices.py
```

The normal command records unresolved source-text gaps explicitly. `--strict` additionally fails when those gaps remain; use it for a publication review rather than treating a draft build as proof that every notice has been obtained. `--inventory` omits the text appendix and is unsuitable as a complete redistributed notice file.

## Capture provenance

The desktop snapshot was captured with `cargo metadata --locked --offline --format-version 1 --filter-platform aarch64-apple-darwin`. Its Cargo.lock and Cargo.toml hashes must match the working tree. The release firmware role pins are read as a literal from `scripts/package_companion_release.py`, without importing that script or opening its private evidence. Any pin or desktop manifest/lock change requires a new inventory capture and review.

The two firmware snapshots were captured in temporary source checkouts at the exact source commits recorded in the inventory, using `cargo metadata --locked --offline --format-version 1 --filter-platform thumbv7em-none-eabihf --all-features`. This includes a conservative superset of dependencies for the bundled roles, including optional crates and build tools. It is not an object-level linkage inventory. Source checkout and history were used only during this audit; they are not needed to render or validate notices in a shallow CI checkout. Local project packages are covered by the repository LICENSE and NOTICE.md.

For each graph, capture the selected root resolve-node dependency closure, upstream name/version/source/license metadata, Cargo.lock archive checksums, and published `.cargo_vcs_info.json` revision where available. Copy all package LICENSE, LICENCE, COPYING, NOTICE, and COPYRIGHT text files, including nested vendored libraries. When a workspace crate omits its notice, copy the repository-root notice from its exact pinned Git checkout. For other missing notices, obtain full source texts only from immutable published-package revisions or resolved version tags. Store the original bytes by SHA-256 and record their immutable URL. Never substitute an unversioned branch or a standard license template for missing upstream copyright notices.

The nrf-pac notice is extracted verbatim from the licenseText XML element of the exact pinned nrf52833 SVD; literal escaped newline sequences are retained.

The firmware snapshots include Nordic `nrf-mpsl-sys` and `nrf-sdc-sys` nested SDK license, attribution, and CMSIS texts, plus the P256 library's packaged notice. The desktop includes `gpui-kit-assets` Lucide asset notices in addition to its Apache license. License expressions reproduce upstream declarations; they do not assert that every alternative license must be selected.

## Remaining source-text gaps

Twelve unique package versions currently lack a packaged or immutable upstream full notice text. The exact list and reasons are generated in the notices file. Immutable revision and package archive checksum provenance are retained where available. No unverified branch text is included as a substitute.

- Desktop: `block 0.1.6`, `leak 0.1.2`, `leaky-cow 0.1.1`, `mac 0.1.1`, `malloc_buf 0.0.6`, `objc_exception 0.1.2`, `pathfinder_geometry 0.5.1`, `seahash 4.1.0`, `simd_helpers 0.1.0`.
- Firmware: `btuuid 0.1.1`, `p256-cortex-m4 0.1.0-alpha.6`, `void 1.0.2`.

`leak` and `leaky-cow` omit a packaged VCS revision and no matching version tag was found. The other source revisions were identified, but a full notice text was not obtained from their immutable upstream source. This is a concrete notice acquisition gap; SPDX declarations remain recorded rather than being replaced with guessed ownership claims.
