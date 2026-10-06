#!/usr/bin/env python3
"""Fetch the pinned upstream sources and apply the migration patches."""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RMK_REVISION = "434ab4d7d29d8e9ba689837358c8a44996ba38cc"
EMBASSY_VERSION = "0.11.0"
EMBASSY_SHA256 = "82051f67e8a06a8faebd183f62cfade184c96be68126c5f064abb8223a591ff1"


def run(*args, cwd):
    subprocess.run(args, cwd=cwd, check=True)


def source_digest(directory):
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*")):
        relative = path.relative_to(directory)
        if ".git" in relative.parts or "target" in relative.parts or path.name == ".migration-sources.json":
            continue
        if path.is_file():
            digest.update(relative.as_posix().encode() + b"\0")
            digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def prepare():
    patches = ROOT / "firmware" / "patches"
    identity = {"rmk_revision": RMK_REVISION, "embassy_sha256": EMBASSY_SHA256}
    for name in ("rmk-minimal.patch", "embassy-preserve-uicr.patch"):
        identity[name] = hashlib.sha256((patches / name).read_bytes()).hexdigest()
    destination = ROOT / "dependencies"
    marker = destination / ".migration-sources.json"
    if destination.exists():
        if marker.is_file():
            recorded = json.loads(marker.read_text())
            if recorded.get("pins") == identity and recorded.get("source_sha256") == source_digest(destination):
                print("Migration sources are already prepared and verified.")
                return
        raise RuntimeError("dependencies already exists with different sources. Move it aside before setup.")

    with tempfile.TemporaryDirectory(prefix=".migration-", dir=ROOT) as temporary:
        staging = Path(temporary)
        rmk = staging / "rmk"
        rmk.mkdir()
        print("Fetch pinned RMK sources.", flush=True)
        run("git", "init", "--quiet", cwd=rmk)
        run("git", "fetch", "--quiet", "--depth=1", "https://github.com/rmk-rs/rmk.git", RMK_REVISION, cwd=rmk)
        run("git", "checkout", "--quiet", "--detach", "FETCH_HEAD", cwd=rmk)
        actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=rmk, text=True).strip()
        if actual != RMK_REVISION:
            raise RuntimeError("RMK revision does not match the pin.")
        run("git", "apply", "--check", str(patches / "rmk-minimal.patch"), cwd=rmk)
        run("git", "apply", str(patches / "rmk-minimal.patch"), cwd=rmk)

        print("Fetch and verify published embassy-nrf sources.", flush=True)
        archive = staging / "embassy-nrf.crate"
        url = f"https://static.crates.io/crates/embassy-nrf/embassy-nrf-{EMBASSY_VERSION}.crate"
        with urllib.request.urlopen(url, timeout=60) as response:
            archive.write_bytes(response.read())
        if hashlib.sha256(archive.read_bytes()).hexdigest() != EMBASSY_SHA256:
            raise RuntimeError("embassy-nrf archive checksum does not match the pin.")
        with tarfile.open(archive) as contents:
            contents.extractall(staging, filter="data")
        embassy = staging / f"embassy-nrf-{EMBASSY_VERSION}"
        run("git", "init", "--quiet", cwd=embassy)
        run("git", "apply", "--check", str(patches / "embassy-preserve-uicr.patch"), cwd=embassy)
        run("git", "apply", str(patches / "embassy-preserve-uicr.patch"), cwd=embassy)
        prepared = staging / "dependencies"
        prepared.mkdir()
        shutil.move(rmk, prepared / "rmk")
        shutil.move(embassy, prepared / "embassy-nrf")
        (prepared / ".migration-sources.json").write_text(
            json.dumps({"pins": identity, "source_sha256": source_digest(prepared)}, indent=2) + "\n"
        )
        prepared.rename(destination)
    print("Migration sources are ready in dependencies/.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true", help="show source pins without downloading or writing files")
    args = parser.parse_args()
    if args.dry_run:
        print(f"RMK {RMK_REVISION}; embassy-nrf {EMBASSY_VERSION} ({EMBASSY_SHA256}).")
        print(f"Apply firmware/patches/ into {ROOT / 'dependencies'}.")
        return
    try:
        prepare()
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Setup failed: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
