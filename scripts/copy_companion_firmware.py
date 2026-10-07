#!/usr/bin/env python3
"""Copy only firmware files named by a validated Companion manifest."""
import argparse
import json
from pathlib import Path
import shutil


def copy(source: Path, destination: Path):
    manifest = json.loads((source / 'manifest.json').read_text(encoding='utf-8'))
    names = {'manifest.json'}
    for image in manifest['images']:
        names.update((image['uf2'], image['binary']))
    for name in names:
        if not name or Path(name).name != name or '\\' in name:
            raise ValueError('Firmware manifest contains an invalid filename')
    for name in names:
        path = source / name
        if not path.is_file() or path.is_symlink():
            raise ValueError(f'Firmware file is unavailable: {name}')
    destination.mkdir(parents=True, exist_ok=True)
    for name in names:
        shutil.copy2(source / name, destination / name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    copy(args.source, args.destination)
