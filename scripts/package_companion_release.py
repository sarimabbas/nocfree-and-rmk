#!/usr/bin/env python3
"""Package the reviewed firmware build."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from migration_guard import inspect_startup_image
from image_guard import inspect as inspect_receiver

VERSION = '1.0.0'
# Reviewed ANSI rebuild from the hardware-tested sources; no device acceptance is claimed.
ROLES = {
    'left': ('native-release-1.0.0/left', 'd07e158a43f2593ad7c8bc7d11feb29f397bc8b5654d1d065e832d97700a26fd', 'c390139c26f6af40bedf7911202ce088c18a53b6485e65a7a92ff28d06395271', '880c967b2e9b51806ef18f40acfe6ee0b8b53c3e', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
    'right': ('native-release-1.0.0/right', '091d5cccfbd239ede9c39e40244941214211251625279f54e1b661a2e14b842c', '0319163343f75b71f91ad6b788d0a468173e7af11b0b380ea653c211850ebb08', '880c967b2e9b51806ef18f40acfe6ee0b8b53c3e', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
    'receiver': ('native-release-1.0.0/receiver', '93071cfa5876eb969a78d9add425568d4cd8a1d283fe3c491d9199696069d5bf', '507dfe384594710e54e58d092b7af42a95e616da48c054fa9ca135a7a1866d18', '880c967b2e9b51806ef18f40acfe6ee0b8b53c3e', '434ab4d7d29d8e9ba689837358c8a44996ba38cc'),
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verified_pair(root, role, spec):
    folder, uf2_hash, bin_hash, commit, revision = spec
    base = root / '.evidence' / folder
    uf2, binary = (base / 'candidate.uf2').read_bytes(), (base / 'candidate.bin').read_bytes()
    if digest(uf2) != uf2_hash or digest(binary) != bin_hash:
        raise ValueError(f'{role}: candidate is outside the release allowlist')
    if f'NocFree RMK;fw={VERSION}'.encode() not in binary:
        raise ValueError(f'{role}: firmware version descriptor differs')
    start = 0x27000 if role == 'receiver' else 0x1000
    padded = binary + b'\xff' * (-len(binary) % 4096)
    if role == 'receiver':
        guard = inspect_receiver(uf2)
        from migration_guard import _payload
        expected = _payload(uf2, start, start + len(padded), 0x621e937a)
        if expected != padded or (int(guard['reset_vector'], 16) & ~1) >= start + len(binary):
            raise ValueError('dongle UF2/BIN or vector mismatch')
    else:
        guard = inspect_startup_image(uf2, binary, role)
    metadata = dict(role=role, layout='ansi', keymap='mac', uf2=f'{role}.uf2', binary=f'{role}.bin', uf2_sha256=uf2_hash, binary_sha256=bin_hash,
                    origin=start, end_exclusive=start + len(padded), binary_size=len(binary),
                    policy='receiver_protected' if role == 'receiver' else f'{role}_startup',
                    source_commit=commit, rmk_revision=revision,
                    battery_calibration='provisional_150_100' if role != 'receiver' else 'none',
                    storage_revision=revision, storage_preserved=False,
                    recovery_evidence='software_checks_only', current_image_runtime_recovery=False,
                    bootloader='0.9.2-39-g0147d71', board_id='NocFree &', family_id=0x621e937a,
                    softdevice='S140 7.3.0' if role == 'receiver' else 'reclaimed')
    return metadata, uf2, binary


def package(root=ROOT, destination=None, check_only=False):
    pairs = [verified_pair(root, role, spec) for role, spec in ROLES.items()]
    release_id = 'nocfree-' + digest(''.join(pair[0]['uf2_sha256'] for pair in pairs).encode())[:16]
    manifest = dict(schema=1, release_id=release_id, version=VERSION, images=[pair[0] for pair in pairs])
    encoded = (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode()
    if not check_only:
        destination = destination or root / 'dist' / 'companion-firmware'
        destination.mkdir(parents=True, exist_ok=True)
        for metadata, uf2, binary in pairs:
            (destination / metadata['uf2']).write_bytes(uf2)
            (destination / metadata['binary']).write_bytes(binary)
        (destination / 'manifest.json').write_bytes(encoded)
    return release_id, digest(encoded)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Validate all evidence without generating files.')
    args = parser.parse_args()
    try:
        release_id, manifest_hash = package(check_only=args.check)
        print(f'{release_id}: verified release manifest SHA256 {manifest_hash}')
    except (ValueError, OSError, KeyError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
