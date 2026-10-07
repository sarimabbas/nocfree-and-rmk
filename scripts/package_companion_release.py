#!/usr/bin/env python3
"""Package the reviewed firmware images without rebuilding or touching devices."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from image_guard import inspect as inspect_receiver
from migration_guard import inspect_startup_image

ROOT = Path(__file__).resolve().parents[1]
VERSION = '1.0.3'
MANIFEST_SHA256 = '3c1a278cf9e18b231c9d59b59389008d7bdab03976ee41c1ca1e9ee9ffcd0466'
SOURCE = '.evidence/native-release-1.0.3/companion'
ROLES = {
    'left': ('native-release-1.0.3/companion', 'cc6d8aa050c184aa2a2c728c8af814f04e2047723612575c9bc64546731e1527', 'a03153b92f9e89f09a0f2ee57c0a1106c7e6105cb7537ec0d1c75296b9a6d868', '4a52e1a5c48fc53803454939b35fdfdc1584b40d', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
    'right': ('native-release-1.0.3/companion', '833cdb336137604dbca0395b9145a78b8594f271be808996c81d9baed3acc4d1', '527c398eec835665d91eca056f1c9bccdf23bab6c28647f07e4cc1ac7dd44aa9', '4a52e1a5c48fc53803454939b35fdfdc1584b40d', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
    'receiver': ('native-release-1.0.3/companion', '3d3f4de4a76b49880924308d6e6929641eda4192abfeeb0fd35511c095042c71', '338323349918a9f3ada58d391ca77ba86f58a6f339da93e3f0c36e751fd339bc', '4a52e1a5c48fc53803454939b35fdfdc1584b40d', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def package(root=ROOT, destination=None, check_only=False):
    source = root / SOURCE
    manifest_bytes = (source / 'manifest.json').read_bytes()
    if digest(manifest_bytes) != MANIFEST_SHA256:
        raise ValueError('Firmware manifest is outside the release allowlist')
    manifest = json.loads(manifest_bytes)
    expected = [(role, layout, f'{role}-{layout}')
                for layout in ('ansi', 'iso', 'jis', 'kr')
                for role in ('left', 'right')]
    expected.append(('receiver', 'ansi', 'receiver'))
    if manifest['version'] != VERSION or len(manifest['images']) != len(expected):
        raise ValueError('Firmware version or image count differs')
    files = {'manifest.json': manifest_bytes}
    for image, (role, layout, stem) in zip(manifest['images'], expected):
        if (image['role'], image['layout'], image['uf2'], image['binary']) != (
                role, layout, f'{stem}.uf2', f'{stem}.bin'):
            raise ValueError('Firmware image order or name differs')
        if (image['source_commit'], image['rmk_revision']) != ROLES[role][3:]:
            raise ValueError('Firmware source differs')
        for name, key in ((image['uf2'], 'uf2_sha256'),
                          (image['binary'], 'binary_sha256')):
            path = source / name
            if path.is_symlink():
                raise ValueError('Firmware file must not be a symlink')
            data = path.read_bytes()
            if digest(data) != image[key]:
                raise ValueError('Firmware file is outside the release allowlist')
            files[name] = data
        if role == 'receiver':
            proof = inspect_receiver(files[image['uf2']])
        else:
            proof = inspect_startup_image(files[image['uf2']], files[image['binary']], role)
        if int(proof['end_exclusive'], 16) != image['end_exclusive']:
            raise ValueError('Firmware address range differs')
        if f'NocFree RMK;fw={VERSION}'.encode() not in files[image['binary']]:
            raise ValueError('Firmware version descriptor differs')
    if not check_only:
        destination = destination or root / 'dist/companion-firmware'
        if destination.exists():
            raise ValueError('Firmware destination already exists')
        destination.mkdir(parents=True)
        for name, data in files.items():
            (destination / name).write_bytes(data)
    return manifest['release_id'], MANIFEST_SHA256


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Validate all evidence without generating files.')
    args = parser.parse_args()
    try:
        release_id, manifest_hash = package(check_only=args.check)
        print(f'{release_id}: verified release manifest SHA256 {manifest_hash}')
    except (ValueError, OSError, KeyError, json.JSONDecodeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
