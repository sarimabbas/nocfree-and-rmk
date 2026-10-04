#!/usr/bin/env python3
"""Package the fixed, hardware-readback-verified local release. Never accesses devices."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from migration_guard import inspect_application_shim
from image_guard import inspect as inspect_receiver

VERSION = '0.1.0-local.1'
# This is an explicit reviewed allowlist, not a glob of whatever was most recently built.
ROLES = {
    'left': ('physical-mode-battery/calibration-left', 'fdc098d69df45152da01559af8f99caf610fb17f86615f7f514aad23d10ca213', '495033e67d8aa150e8d7a186263ec673c2acff99c1953825e6ff3f2d6a05feec', '2438a73ffc6d4b2c18627c54d6bb3a0b1a06452b', 'acd4689a1284f27decd4d0755a1e316fddcbb8ff'),
    'right': ('physical-mode-battery/calibration-right', '5380a1bab323ddc3b9cabc578b2fbe6428c826d4eb460971a4a0e67582d8be2f', '1f4824e0b34c14b018c6b0a58f64c5b4232c2719836d3d337d52f9921153e5a7', '77f75a2034bda0a5944e15427bed6e396ce71889', 'c8e06c6edbda227114ab7deeb9234d2ecca671c6'),
    'receiver': ('vial-battery-review/receiver-trial', 'd2caae3554b36cba0579f0bad0d60404040e8e007fe5002e14445c9e935109e7', '150f76188fd8e388a19cc533a1c45d66f9928632dcff28c89d887099eac2692b', '447990593cf7982a02fdafb5dc0fc62bbb969e53', 'c8e06c6edbda227114ab7deeb9234d2ecca671c6'),
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_text())


def verified_pair(root, role, spec):
    folder, uf2_hash, bin_hash, commit, revision = spec
    base = root / '.evidence' / folder
    uf2, binary = (base / 'candidate.uf2').read_bytes(), (base / 'candidate.bin').read_bytes()
    if digest(uf2) != uf2_hash or digest(binary) != bin_hash:
        raise ValueError(f'{role}: candidate is outside the release allowlist')
    observed = read_json(base / 'verified.json')
    review = read_json(base / 'candidate-review.json')
    if observed.get('schema') != 1 or observed.get('application_exact') is not True or review.get('schema') != 1 or review.get('passed') is not True or review.get('role') != role:
        raise ValueError(f'{role}: missing exact readback or role-specific review')
    if review.get('candidate_sha256', review.get('uf2_sha256')) != uf2_hash or review.get('binary_sha256') != bin_hash:
        raise ValueError(f'{role}: review does not bind these images')
    installed = (base / 'installed-CURRENT.UF2').read_bytes()
    payload = {}
    for offset in range(0, len(installed), 512):
        block = installed[offset:offset + 512]
        if len(block) != 512:
            raise ValueError('truncated installed readback')
        address, length = struct.unpack_from('<II', block, 12)
        if length != 256 or address in payload:
            raise ValueError('ambiguous installed readback')
        payload[address] = block[32:288]
    start = 0x27000 if role == 'receiver' else 0x1000
    if role == 'receiver':
        guard = inspect_receiver(uf2)
        padded = binary + b'\xff' * (-len(binary) % 4096)
        blocks = sorted((struct.unpack_from('<I', uf2, i+12)[0], uf2[i+32:i+288]) for i in range(0, len(uf2), 512))
        expected = b''.join(data for _, data in blocks)
        if expected != padded:
            raise ValueError('receiver UF2/BIN mismatch')
        recovery = read_json(base / 'runtime-recovery-observation.json')
        if recovery.get('schema') != 1 or not all(observed.get(k) is True for k in ('S140_prefix_exact', 'untouched_application_gap_exact', 'storage_change_expected_by_schema')) or not all(recovery.get(k) is True for k in ('actual_companion_receiver_recovery', 'exact_installed_readback')):
            raise ValueError('receiver preservation/recovery evidence missing')
    else:
        guard = inspect_application_shim(uf2, binary, role)
        padded = binary + b'\xff' * (-len(binary) % 4096)
        acceptance = read_json(base / 'acceptance-checkpoint.json')
        if acceptance.get('schema') != 1 or not all(observed.get(k) is True for k in ('padding_exact', 'untouched_gap_exact', 'storage_equal', 'recovery_drive_readback_observed')) or acceptance.get('companion_runtime_recovery_observed') is not True:
            raise ValueError(f'{role}: preservation/recovery evidence missing')
    actual = b''.join(payload.get(address, b'') for address in range(start, start + len(padded), 256))
    if actual != padded:
        raise ValueError(f'{role}: stored readback does not match candidate')
    metadata = dict(role=role, layout='ansi', keymap='mac', uf2=f'{role}.uf2', binary=f'{role}.bin', uf2_sha256=uf2_hash, binary_sha256=bin_hash,
                    origin=start, end_exclusive=start + len(padded), binary_size=len(binary),
                    policy='receiver_protected' if role == 'receiver' else f'{role}_startup',
                    source_commit=commit, rmk_revision=revision,
                    battery_calibration='provisional_150_100' if role != 'receiver' else 'none',
                    storage_revision=revision,
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
