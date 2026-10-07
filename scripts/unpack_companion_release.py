#!/usr/bin/env python3
"""Extract only a checksum-pinned reviewed firmware package; never accesses devices."""
import argparse
import hashlib
from pathlib import Path
import zipfile

FILES = {'manifest.json', *(f'{role}.{ext}' for role in ('left', 'right', 'receiver') for ext in ('bin', 'uf2'))}
ALL_LAYOUT_FILES = {'manifest.json', 'receiver.bin', 'receiver.uf2', *(
    f'{role}-{layout}.{ext}'
    for role in ('left', 'right')
    for layout in ('ansi', 'iso', 'jis', 'kr')
    for ext in ('bin', 'uf2')
)}

def unpack(source, checksum, destination):
    raw = source.read_bytes()
    if len(raw) > 8 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != checksum:
        raise ValueError('Firmware ZIP size or SHA256 does not match the reviewed input.')
    with zipfile.ZipFile(source) as archive:
        entries = archive.infolist()
        names = {e.filename for e in entries}
        if len(entries) != len(names) or names not in (FILES, ALL_LAYOUT_FILES):
            raise ValueError('Firmware ZIP must contain exactly one reviewed package at its root.')
        if any(e.file_size > 1024 * 1024 or e.is_dir() or e.flag_bits & 1 for e in entries):
            raise ValueError('Firmware ZIP has an oversized, encrypted or directory entry.')
        if destination.exists():
            raise ValueError('Firmware destination already exists; use a fresh directory.')
        destination.mkdir(parents=True)
        for entry in entries:
            (destination / entry.filename).write_bytes(archive.read(entry))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        unpack(args.input, args.sha256, args.output)
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        parser.exit(1, f'{error}\n')
