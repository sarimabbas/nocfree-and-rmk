#!/usr/bin/env python3
"""Build a LEFT-only ELF/BIN from two matching original bootloader reads. No flash or packaging."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess

from build_smoke import EXPERIMENT, PINNED, ROOT, run


def checked_evidence(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    if (manifest.get("schema"), manifest.get("role"), manifest.get("observations")) != (1, "left", 2):
        raise ValueError("requires schema 1 LEFT evidence with two matching observations")
    regions = {}
    for name, size in [("upper", 0xC000), ("uicr", 0x308)]:
        data = (directory / f"{name}.bin").read_bytes()
        if len(data) != size or hashlib.sha256(data).hexdigest() != manifest["regions"][name]["sha256"]:
            raise ValueError(f"{name} evidence size/hash mismatch")
        regions[name] = data
    if struct.unpack_from("<II", regions["uicr"], 0x14) != (0x74000, 0x7E000):
        raise ValueError("original UICR does not match the pinned nRF52833 layout")
    cf2 = regions["upper"][0x9800:0xA000]
    expected = (513675505, 539130489, 5, 100, 204, 0x80000, 205, 0x20000,
                208, 0x239A0029, 209, 0x621E937A, 210, 0x20)
    if struct.unpack_from("<14I", cf2) != expected:
        raise ValueError("original CF2 is not the verified LEFT configuration")
    return cf2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, required=True,
                        help="private original-left-readback directory")
    parser.add_argument("--upstream", type=Path,
                        default=ROOT / ".evidence/bootloader-work/upstream")
    parser.add_argument("--toolchain", type=Path, required=True,
                        help="directory containing arm-none-eabi tools")
    parser.add_argument("--output", type=Path, required=True,
                        help="new private build directory (must not exist)")
    args = parser.parse_args()
    cf2 = checked_evidence(args.evidence)
    upstream = args.upstream.resolve()
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=upstream, text=True).strip() != PINNED:
        parser.error(f"upstream must be pinned to {PINNED}")
    statuses = subprocess.check_output(["git", "submodule", "status"], cwd=upstream, text=True).splitlines()
    if len(statuses) != 3 or any(not line.startswith(" ") for line in statuses):
        parser.error("requires three initialized pinned submodules")
    for source in [upstream] + [upstream / line.split()[1] for line in statuses]:
        if subprocess.check_output(["git", "status", "--porcelain"], cwd=source):
            parser.error("source and submodules must be clean")
    if (upstream / "Makefile.user").exists():
        parser.error("upstream must not contain Makefile.user")
    toolchain = args.toolchain.resolve()
    if not all((toolchain / f"arm-none-eabi-{tool}").is_file() for tool in ["gcc", "objcopy", "size"]):
        parser.error("toolchain must contain gcc, objcopy and size")
    output = args.output.resolve()
    if output.exists():
        parser.error("output must be a new directory")
    shutil.copytree(upstream, output,
                    ignore=shutil.ignore_patterns(".git", "_build", "_bin", "Makefile.user"))
    output.chmod(0o700)
    run("patch", "-p1", "-F0", "-i", str(EXPERIMENT / "bootloader.patch"), cwd=output)
    run("python3", str(EXPERIMENT / "test_integration.py"), "--main", str(output / "src/main.c"))
    board = output / "src/boards/nocfree_left"
    shutil.copytree(EXPERIMENT / "board-left", board)
    words = struct.unpack("<512I", cf2)
    (board / "pinconfig.c").write_text(
        '#include "boards.h"\n__attribute__((used, section(".bootloaderConfig")))\n'
        'const uint32_t bootloaderConfig[] = {\n' +
        "\n".join("  " + ", ".join(f"0x{word:08x}" for word in words[i:i + 8]) + ","
                  for i in range(0, len(words), 8)) + "\n};\n")
    sources = output / "src/nocfree"
    sources.mkdir()
    for name in ["recovery_gate.c", "recovery_gate.h", "nrf_startup.c", "nrf_startup.h"]:
        shutil.copy2(EXPERIMENT / name, sources / name)
    (output / "Makefile.user").write_text(
        "C_SRC += src/nocfree/recovery_gate.c src/nocfree/nrf_startup.c\n"
        "IPATH += src/nocfree\nCFLAGS += -DNOCFREE_RECOVERY_ROLE=RECOVERY_LEFT\n"
        # Preserve the already configured original UICR. Upstream SystemInit
        # otherwise contains reset/NFC reprogramming paths even if today's
        # matching values happen to skip those writes.
        "_build/build-nocfree_left/system_nrf52833.o: CFLAGS += -UCONFIG_NFCT_PINS_AS_GPIOS -UCONFIG_GPIO_AS_PINRESET\n"
        "_build/build-nocfree_left/bootloader_settings.o: CFLAGS += -Wno-error=array-bounds\n"
        "_build/build-nocfree_left/ghostfat.o: CFLAGS += -Wno-error=unterminated-string-initialization\n")
    elf = output / "_build/build-nocfree_left/nocfree_left_bootloader-0.9.2.out"
    run("make", "-j4", "BOARD=nocfree_left", "GIT_VERSION=0.9.2",
        f"CROSS_COMPILE={toolchain / 'arm-none-eabi-'}", str(elf.relative_to(output)), cwd=output)
    shutil.copy2(elf, output / "candidate.elf")
    # UICR sections are only linker metadata. Never export them as binary content.
    run(str(toolchain / "arm-none-eabi-objcopy"), "-O", "binary", "--gap-fill=0xff",
        "--remove-section=.uicrBootStartAddress", "--remove-section=.uicrMbrParamsPageAddress",
        str(elf), str(output / "candidate.bin"))
    binary = (output / "candidate.bin").read_bytes()
    if len(binary) != 0xA000 or binary[0x9800:] != cf2:
        raise ValueError("candidate must span 0x74000..0x7e000 with exact original CF2")
    run(str(toolchain / "arm-none-eabi-size"), str(elf))
    for name in ["candidate.elf", "candidate.bin"]:
        (output / name).chmod(0o600)
    print(f"LEFT-only candidate built in {output}; no package or device operation")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Build failed: {error}") from None
