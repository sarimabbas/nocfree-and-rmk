#!/usr/bin/env python3
"""Build-only integration check. Uses a Nordic DK config, never a NocFree image."""
import argparse
from pathlib import Path
import shutil
import subprocess

PINNED = "0147d71e73b9a2c217f56dbc9877d07bb45d6467"
EXPERIMENT = Path(__file__).resolve().parent
ROOT = EXPERIMENT.parents[1]


def run(*command, cwd=None):
    subprocess.run(command, cwd=cwd, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", "-u", type=Path,
                        default=ROOT / ".evidence/bootloader-work/upstream",
                        help="clean pinned Adafruit checkout with initialized submodules")
    parser.add_argument("--toolchain", "-t", type=Path,
                        help="directory containing arm-none-eabi-gcc (default: PATH)")
    parser.add_argument("--role", "-r", choices=["left", "right", "both"], default="both")
    args = parser.parse_args()
    upstream = args.upstream.resolve()
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=upstream, text=True).strip()
    if revision != PINNED:
        parser.error(f"upstream must be pinned to {PINNED}")
    run("git", "diff", "--exit-code", "HEAD", cwd=upstream)
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=upstream):
        parser.error("upstream checkout must have no tracked or untracked changes")
    if (upstream / "Makefile.user").exists():
        parser.error("remove local Makefile.user from the source checkout")
    statuses = subprocess.check_output(
        ["git", "submodule", "status"], cwd=upstream, text=True).splitlines()
    if len(statuses) != 3 or any(not line.startswith(" ") for line in statuses):
        parser.error("initialize all three pinned submodules before building")
    for line in statuses:
        run("git", "diff", "--exit-code", "HEAD", cwd=upstream / line.split()[1])
        if subprocess.check_output(["git", "status", "--porcelain"],
                                   cwd=upstream / line.split()[1]):
            parser.error("submodules must have no tracked or untracked changes")
    gcc = (args.toolchain.resolve() / "arm-none-eabi-gcc" if args.toolchain
           else Path(shutil.which("arm-none-eabi-gcc") or ""))
    if not gcc.is_file():
        parser.error("arm-none-eabi-gcc unavailable; supply --toolchain")
    roles = ["left", "right"] if args.role == "both" else [args.role]
    for role in roles:
        print(f"Building {role} bootloader integration with DK configuration; not installable", flush=True)
        destination = ROOT / ".evidence/bootloader-work" / f"smoke-{role}"
        if destination.exists():
            parser.error(f"remove the previous smoke checkout before rebuilding: {destination}")
        shutil.copytree(upstream, destination,
                        ignore=shutil.ignore_patterns(".git", "_build", "_bin", "Makefile.user"))
        # The snapshot has no .git directory. git apply from here can discover
        # the parent project and silently skip paths outside that subdirectory.
        # Apply directly to the snapshot, with no fuzzy hunk matching.
        run("patch", "-p1", "-F0", "-i", str(EXPERIMENT / "bootloader.patch"),
            cwd=destination)
        if "local_key_dfu = recovery_nrf_startup" not in (destination / "src/main.c").read_text():
            parser.error("startup patch was not applied to the smoke snapshot")
        run("python3", str(EXPERIMENT / "test_integration.py"), "--main",
            str(destination / "src/main.c"))
        sources = destination / "src/nocfree"
        sources.mkdir()
        for name in ["recovery_gate.c", "recovery_gate.h", "nrf_startup.c", "nrf_startup.h"]:
            shutil.copy2(EXPERIMENT / name, sources / name)
        (destination / "Makefile.user").write_text(
            "C_SRC += src/nocfree/recovery_gate.c src/nocfree/nrf_startup.c\n"
            "IPATH += src/nocfree\n"
            f"CFLAGS += -DNOCFREE_RECOVERY_ROLE=RECOVERY_{role.upper()}\n"
            # SDK11 intentionally dereferences fixed low-flash MBR addresses.
            # GCC 15 diagnoses those as zero-sized arrays. Keep all warnings
            # fatal in our code; permit this warning in that upstream file only.
            "_build/build-pca10100/bootloader_settings.o: CFLAGS += -Wno-error=array-bounds\n"
            # UF2 FAT names deliberately occupy all 11 bytes without a NUL.
            "_build/build-pca10100/ghostfat.o: CFLAGS += -Wno-error=unterminated-string-initialization\n")
        # Explicit ELF target only: do not invoke upstream 'all', packaging or flash targets.
        run("make", "-j4", "BOARD=pca10100", "GIT_VERSION=0.9.2",
            f"CROSS_COMPILE={gcc.parent / 'arm-none-eabi-'}",
            "_build/build-pca10100/pca10100_bootloader-0.9.2.out", cwd=destination)
        print(f"Built {role} ELF in {destination}; no firmware package generated", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Build failed: {error}") from None
