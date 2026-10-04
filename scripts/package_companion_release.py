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

VERSION = '0.1.0-local.2'
# This is an explicit reviewed allowlist, not a glob of whatever was most recently built.
ROLES = {
    'left': ('dongle-pairing/live-candidates/left', '85628cb1cf03487c75828b28ac46071c363efbce7fac73867aa01306106b8111', '4a6128fc7fdb264dde3f1d0ab2b391ddf0a1cea15acddfcbbdef7759aa8f6a48', '94f83b59617b9323bbc484d2440b487848291422', '0afc58c9db423313faa0c47021fc7957f4416a11', 'dongle-pairing/operator-review.json', 'physical-mode-battery/calibration-left/acceptance-checkpoint.json'),
    'right': ('lighting-state-fix/live-candidates/right', 'af47b51428f00c54c16dfd157d46fc7b4bb062b98b6ecd4a626d05511c377d31', 'a8db6c2d190de01b0244ded2fd02e85306d1df1462b13727fd0f8794f1026b35', '1acd812a82e0ae3e99a1d498af52c0e1f37fa916', '33214e095a86e21bac6184ab9e4d3565ba72a849', 'lighting-state-fix/operator-right-review.json', 'vial-battery-review/right-trial/runtime-recovery-observation.json'),
    'receiver': ('dongle-pairing/live-candidates/receiver', '7199961f706f3b074d03a076944768099b0c47e74b875dce1c7f8b8e4d7bac83', '470e5b718edae54740fc682c0ce28f96c211ac06f0d6afc73e90f3e6c705b438', '94f83b59617b9323bbc484d2440b487848291422', '0afc58c9db423313faa0c47021fc7957f4416a11', 'dongle-pairing/operator-review.json', 'vial-battery-review/receiver-trial/runtime-recovery-observation.json'),
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
    recovery_key = {'left': 'companion_runtime_recovery_observed', 'right': 'companion_runtime_recovery_pass', 'receiver': 'actual_companion_receiver_recovery'}[role]
    if recovery.get('schema') != 1 or recovery.get(recovery_key) is not True:
        raise ValueError(f'{role}: retained role-specific recovery route unproven')
    metadata = dict(role=role, layout='ansi', keymap='mac', uf2=f'{role}.uf2', binary=f'{role}.bin', uf2_sha256=uf2_hash, binary_sha256=bin_hash,
                    origin=start, end_exclusive=start + len(padded), binary_size=len(binary),
                    policy='receiver_protected' if role == 'receiver' else f'{role}_startup',
                    source_commit=commit, rmk_revision=revision,
                    battery_calibration='provisional_150_100' if role != 'receiver' else 'none',
                    storage_revision=revision, storage_preserved=storage_preserved,
                    recovery_evidence='retained_role_specific_route_unchanged', current_image_runtime_recovery=False,
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
