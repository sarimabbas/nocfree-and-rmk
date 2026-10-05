# Dependency notice sources

`inventory.json` records the resolved desktop dependencies and the pinned firmware source graph shared by all three roles. `texts/` contains unmodified UTF-8 upstream notices, original published license declarations, and explicitly labelled supplementary license terms named by their SHA-256. `desktop/THIRD_PARTY_NOTICES.md` is the checked-in catalog; the bundle generator reproduces complete text in a deduplicated appendix. Newline conventions are normalized only in the generated Markdown, not in the preserved source files.

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

The firmware snapshot was captured at source commit `fb45eecef29f2a2883f44cd6e28843798238d0a7`, with RMK revision `89fead1de856132911ec685313fa88cda0652bac`, using `cargo metadata --locked --offline --format-version 1 --filter-platform thumbv7em-none-eabihf --all-features`. This includes a conservative superset of dependencies for the bundled roles, including optional crates and build tools. It is not an object-level linkage inventory. Source checkout and history were used only during this audit; they are not needed to render or validate notices in a shallow CI checkout. Local project packages are covered by the repository LICENSE and NOTICE.md.

For each graph, capture the selected root resolve-node dependency closure, upstream name/version/source/license metadata, Cargo.lock archive checksums, and published `.cargo_vcs_info.json` revision where available. Copy all package LICENSE, LICENCE, COPYING, NOTICE, and COPYRIGHT text files, including nested vendored libraries. When a workspace crate omits its notice, copy the repository-root notice from its exact pinned Git checkout. For other missing notices, obtain full source texts only from immutable published-package revisions or resolved version tags. Store the original bytes by SHA-256 and record their immutable URL. Never substitute an unversioned branch or a standard license template for missing upstream copyright notices.

The nrf-pac notice is extracted verbatim from the licenseText XML element of the exact pinned nrf52833 SVD; literal escaped newline sequences are retained.

The firmware snapshots include Nordic `nrf-mpsl-sys` and `nrf-sdc-sys` nested SDK license, attribution, and CMSIS texts, plus the P256 library's packaged notice. The desktop includes `gpui-kit-assets` Lucide asset notices in addition to its Apache license. License expressions reproduce upstream declarations; they do not assert that every alternative license must be selected.

## Published archives that omit license files

Nine package versions declare their license but omit a complete license file. The catalog preserves this omission explicitly. For these packages we downloaded the immutable published crate archive, verified its Cargo.lock checksum, preserved its original `Cargo.toml.orig` (or original `Cargo.toml` when no `.orig` exists), and preserved every distinct source copyright/license header. The original author attribution and license declaration remain verbatim.

- Desktop: `block 0.1.6`, `leak 0.1.2`, `leaky-cow 0.1.1`, `mac 0.1.1`, `objc_exception 0.1.2`, `pathfinder_geometry 0.5.1`, `seahash 4.1.0`.
- Firmware: `btuuid 0.1.1`, `p256-cortex-m4 0.1.0-alpha.6`.

We separately supply the canonical MIT or Apache-2.0 terms from SPDX License List Data v3.28.0, immutable commit `c4a7237ec8f4654e867546f9f409749300f1bf4c`. For dual licenses we select Apache-2.0. These are labelled supplementary canonical terms, never claimed to be an upstream project's copyright notice. Template placeholders remain template text; we do not invent copyright owners or years. The renderer validates the terms against the audited hashes, the selected license against the original TOML declaration, and the presence of archive provenance and an explicit omission note. `--strict` still fails for unresolved acquisitions or absent evidence; it does not waive these checks.

For `malloc_buf`, `simd_helpers`, and `void`, the repository owners subsequently supplied full license files. We preserve the immutable clarification revisions (`d9a3e539642bd90e07df458d226b19cdfa606863`, `82040194cd05affb060bf94d6f19f82a771d07fb`, and `ea5a2526d7a81ff45960525b62a3dbfd34c1703c`) and explicitly state that these texts were supplied after the packaged revision. No unversioned branch text is substituted.

`leak` and `leaky-cow` omit a packaged VCS revision. The published `pathfinder_geometry` and `seahash` `.cargo_vcs_info.json` values contain 39-character revisions; these original values remain recorded without fabricating a missing character. Their published archive checksums, license declarations, and available original copyright headers provide the immutable provenance used here.
