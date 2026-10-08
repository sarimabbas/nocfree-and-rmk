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
VERSION = '1.1.0'
MANIFEST_SHA256 = 'f99e2fe2b748ef3ee01c2804ca5d0aefa7a07ae0cdceff544ad232a9a6b791f3'
SOURCE = '.evidence/native-release-1.1.0/companion'
ROLES = {
    'left': ('native-release-1.1.0/companion', '592818dde1be286d4ed5b87e6e190cce875e5bacf78dfe69d5c3275ed35ae58a', '0669cd1c0cae13ae90e2fe2278409f83640674f3997a43e72e7e792b0ca03cbd', '6ddd1f942a986a0b8e268339e36113de9c73ce11', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
    'right': ('native-release-1.1.0/companion', '74b46f6a881f1da69a3eb883c22d2a0d829a04dc7c089583a2031b7ec86fb5b8', '373ba1405ca84ecdcecbc076588bba971ed179912a54c7eec86426c939250722', '6ddd1f942a986a0b8e268339e36113de9c73ce11', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
    'receiver': ('native-release-1.1.0/companion', '4dafd352037eb5bf1ed5c024992b1859a7a0b4587c50558e04e781eb3e6b5353', 'f8cb36d982f5a9b8d3bd1033aab59f31274734b5da08a416eabf03290cae8ca9', '6ddd1f942a986a0b8e268339e36113de9c73ce11', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
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
