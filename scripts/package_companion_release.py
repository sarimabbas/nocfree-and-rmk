#!/usr/bin/env python3
"""Package the fixed, hardware-readback-verified local release. Never accesses devices."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from migration_guard import inspect_application_shim
from image_guard import inspect as inspect_receiver

VERSION = '0.1.1'
# Explicit reviewed allowlist; never select whichever image was most recently built.
ROLES = {
    'left': ('main-firmware-update/candidates/left', 'c72333620a85e76323225d0d97eb96f69c3448f7102ccfdade8918d5c148bdfb', '9450c8151c2d72107cb42d09aa4c589e6b578faa85a7c1958313d08b52717598', 'fb45eecef29f2a2883f44cd6e28843798238d0a7', '89fead1de856132911ec685313fa88cda0652bac', 'main-firmware-update/operator-review-original-left.json', 'main-firmware-update/candidates/left/runtime-recovery-observation.json'),
    'right': ('main-firmware-update/candidates/right', 'ce2de33edfcf8f3c896aba71a0a70c77df268628bbbf1599018d3955233a2bc1', '035da3c8b97ec623305db4b10ffa535eacf8ec12f90097c336b14ca0fe934474', 'fb45eecef29f2a2883f44cd6e28843798238d0a7', '89fead1de856132911ec685313fa88cda0652bac', 'main-firmware-update/operator-review.json', 'main-firmware-update/candidates/right/runtime-recovery-observation.json'),
    'receiver': ('main-firmware-update/candidates/receiver', '3e26410a96d69017bc2ff7c23a45e45a06ad4f93fea6e3c5cd7719dbec7a7c49', '57d62baa2571327505dba502580a8dacc1e7af7b60cfb7c3cdf7345f229c6ccd', 'fb45eecef29f2a2883f44cd6e28843798238d0a7', '89fead1de856132911ec685313fa88cda0652bac', 'main-firmware-update/operator-review.json', 'main-firmware-update/candidates/receiver/runtime-recovery-observation.json'),
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_text())


def verified_pair(root, role, spec):
    folder, uf2_hash, bin_hash, commit, revision, review_path, recovery_path = spec
    base = root / '.evidence' / folder
    uf2, binary = (base / 'candidate.uf2').read_bytes(), (base / 'candidate.bin').read_bytes()
    if digest(uf2) != uf2_hash or digest(binary) != bin_hash:
        raise ValueError(f'{role}: candidate is outside the release allowlist')
    observed = read_json(base / 'verified.json')
    review = read_json(root / '.evidence' / review_path)
    baseline = read_json(base / 'baseline.json')
    if observed.get('schema') != 1 or observed.get('application_exact') is not True or observed.get('padding_and_untouched_gap_exact') is not True:
        raise ValueError(f'{role}: missing exact installed readback')
    if review.get('schema') != 1 or review.get('passed') is not True or review.get('roles', {}).get(role) != uf2_hash:
        raise ValueError(f'{role}: missing hash-bound role-specific review')
    if baseline.get('schema') != 1 or baseline.get('role') != role or baseline.get('candidate_sha256') != uf2_hash or baseline.get('binary_sha256') != bin_hash or baseline.get('operator_sha256') != review.get('operator_sha256'):
        raise ValueError(f'{role}: review does not bind the candidate and original backup')
    if review.get('baselines', {}).get(role) != digest((base / 'baseline.json').read_bytes()):
        raise ValueError(f'{role}: review does not bind this baseline record')
    before = (base / 'before-CURRENT.UF2').read_bytes()
    installed = (base / 'installed-CURRENT.UF2').read_bytes()
    if digest(before) != baseline.get('baseline_sha256'):
        raise ValueError(f'{role}: original backup changed')
    # Validate the complete archived UF2 layout as well as the application pair.
    # These are stored observations only: no device operations are performed.
    from migration_guard import _payload
    old = _payload(before, 0x1000, 0x6d000, 0x239a0029)
    actual = _payload(installed, 0x1000, 0x6d000, 0x239a0029)
    start = 0x27000 if role == 'receiver' else 0x1000
    padded = binary + b'\xff' * (-len(binary) % 4096)
    if role == 'receiver':
        guard = inspect_receiver(uf2)
        expected = _payload(uf2, start, start + len(padded), 0x621e937a)
        if expected != padded or (int(guard['reset_vector'], 16) & ~1) >= start + len(binary):
            raise ValueError('dongle UF2/BIN or vector mismatch')
    else:
        guard = inspect_application_shim(uf2, binary, role)
    offset, end = start - 0x1000, start - 0x1000 + len(padded)
    if actual[offset:end] != padded or actual[:offset] != old[:offset] or actual[end:0x64000] != old[end:0x64000]:
        raise ValueError(f'{role}: application, padding or preserved bytes differ')
    storage_preserved = actual[0x64000:] == old[0x64000:]
    if observed.get('storage_equal') is not storage_preserved:
        raise ValueError(f'{role}: storage observation differs from archived bytes')
    recovery = read_json(root / '.evidence' / recovery_path)
    if (recovery.get('schema') != 1 or recovery.get('role') != role
            or recovery.get('current_image_runtime_recovery') is not True
            or recovery.get('candidate_sha256') != uf2_hash
            or recovery.get('installed_readback_sha256') != digest(installed)):
        raise ValueError(f'{role}: current-image recovery observation is missing or unbound')
    metadata = dict(role=role, layout='ansi', keymap='mac', uf2=f'{role}.uf2', binary=f'{role}.bin', uf2_sha256=uf2_hash, binary_sha256=bin_hash,
                    origin=start, end_exclusive=start + len(padded), binary_size=len(binary),
                    policy='receiver_protected' if role == 'receiver' else f'{role}_startup',
                    source_commit=commit, rmk_revision=revision,
                    battery_calibration='provisional_150_100' if role != 'receiver' else 'none',
                    storage_revision=revision, storage_preserved=storage_preserved,
                    recovery_evidence='current_image_companion_runtime_recovery_observed', current_image_runtime_recovery=True,
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
