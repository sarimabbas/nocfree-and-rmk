"""Read-only collector for the explicitly requested bootloader inspection probe."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import time

SPANS = {"mbr": (0, 0x1000), "upper": (0x74000, 0x80000),
         "uicr": (0x10001000, 0x10001308)}
HEADER = b"BOOT_READ_V1 "
END = b"END BOOT_READ_V1\n"
RECORD = re.compile(rb"([0-9A-F]{8}):([0-9A-F]{8})\n")


def parse_readback(lines, role):
    """Accept exactly one complete ordered transcript; no guessed/missing words."""
    iterator = iter(lines)
    if next(iterator, None) != HEADER + role.upper().encode() + b"\n":
        raise ValueError("wrong inspection role or protocol header")
    regions = {}
    for name, (start, end) in SPANS.items():
        data = bytearray()
        for address in range(start, end, 4):
            match = RECORD.fullmatch(next(iterator, b""))
            if match is None or int(match[1], 16) != address:
                raise ValueError(f"missing, malformed or unordered word at {address:#x}")
            data.extend(struct.pack("<I", int(match[2], 16)))
        regions[name] = bytes(data)
    if next(iterator, None) != END or next(iterator, None) is not None:
        raise ValueError("missing completion frame or trailing data")
    return regions


def read_lines(port):
    deadline = time.monotonic() + 90
    pending = bytearray()
    lines = []
    while time.monotonic() < deadline:
        chunk = port.read(64)
        if not chunk:
            continue
        pending.extend(chunk)
        while b"\n" in pending:
            line, _, tail = pending.partition(b"\n")
            pending = bytearray(tail)
            line = bytes(line) + b"\n"
            lines.append(line)
            if len(lines) > 13508 or len(line) > 64:
                raise ValueError("inspection response exceeded its fixed bounds")
            if line == END:
                if pending:
                    raise ValueError("unexpected data after completion")
                return lines
        if len(pending) > 64:
            raise ValueError("inspection line exceeded its fixed bound")
    raise ValueError("inspection timed out; partial data is not a backup")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", "-p", required=True, help="inspection probe's serial port")
    parser.add_argument("--role", "-r", required=True, choices=["left", "right"])
    parser.add_argument("--output", "-o", type=Path, required=True,
                        help="new private backup directory; existing paths are rejected")
    args = parser.parse_args()
    try:
        import serial
        from serial.tools import list_ports
    except ImportError:
        parser.error("install pyserial in your private tool environment")
    expected_pid = 0x4670 if args.role == "left" else 0x4671
    matches = [p for p in list_ports.comports() if p.device == args.port]
    if len(matches) != 1 or (matches[0].vid, matches[0].pid) != (0x4C4B, expected_pid):
        parser.error("port is not the selected role's bootloader inspection probe")
    if args.output.exists():
        parser.error("output path already exists; choose a new backup directory")
    snapshots = []
    with serial.Serial(args.port, 115200, timeout=0.25, write_timeout=2) as port:
        for index in range(2):
            print(f"Reading {args.role} bootloader snapshot {index + 1}/2…", flush=True)
            port.write(b"READ_BOOT_V1\n")
            port.flush()
            snapshots.append(parse_readback(read_lines(port), args.role))
    if snapshots[0] != snapshots[1]:
        raise ValueError("readbacks differ; no verified backup saved")
    args.output.mkdir(parents=True, mode=0o700, exist_ok=False)
    manifest = {"schema": 1, "role": args.role, "observations": 2, "regions": {}}
    for name, data in snapshots[0].items():
        target = args.output / f"{name}.bin"
        with target.open("xb") as stream:
            target.chmod(0o600)
            stream.write(data)
        start, end = SPANS[name]
        manifest["regions"][name] = {"start": hex(start), "end_exclusive": hex(end),
                                     "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    manifest["status"] = "two matching reads; not a flash package or proven restore route"
    target = args.output / "manifest.json"
    with target.open("x") as stream:
        target.chmod(0o600)
        json.dump(manifest, stream, indent=2)
    print(f"Saved two matching readbacks to {args.output}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        raise SystemExit(str(error)) from None
