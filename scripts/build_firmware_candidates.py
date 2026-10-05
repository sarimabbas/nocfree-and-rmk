#!/usr/bin/env python3
"""Build guarded, unapproved firmware candidates without accessing devices."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tomllib

import image_guard
import migration_guard

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'thumbv7em-none-eabihf'
ROLES = ('left', 'right', 'receiver')


def features(role):
    result = ['defmt-logging', role, 'mac-keymap', 'runtime-recovery']
    if role != 'receiver':
        result += ['reclaimed-softdevice', 'backlight-active-high', 'async-scanner']
    if role == 'left':
        result += ['status-led']
    return result


def encode(binary, start):
    payload = binary + b'\xff' * (-len(binary) % 4096)
    count = len(payload) // 256
    result = bytearray()
    for index in range(count):
        block = bytearray(512)
        struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                         start + index * 256, 256, index, count, image_guard.FAMILY)
        block[32:288] = payload[index * 256:(index + 1) * 256]
        struct.pack_into('<I', block, 508, 0x0AB16F30)
        result += block
    return bytes(result)


def guard(role, image, binary):
    if role != 'receiver':
        return migration_guard.inspect_application_shim(image, binary, role)
    result = image_guard.inspect(image)
    payload = migration_guard._payload(image, image_guard.START,
                                       int(result['end_exclusive'], 16), image_guard.FAMILY)
    if payload != binary + b'\xff' * (-len(binary) % 4096):
        raise ValueError('receiver UF2 does not match its exact BIN')
    if int(result['reset_vector'], 16) & ~1 >= image_guard.START + len(binary):
        raise ValueError('receiver reset vector exceeds its exact BIN')
    if struct.unpack_from('<I', binary, 0x200)[0] == migration_guard.RECOVERY_MARKER:
        raise ValueError('receiver must not contain recovery-first marker')
    return result


def digest(data):
    return hashlib.sha256(data).hexdigest()


def capture(command, cwd=ROOT):
    return subprocess.check_output(command, cwd=cwd, text=True).strip()


def llvm_tool(name, toolchain):
    # LLVM tools may live in stable while the firmware compiler remains pinned.
    for channel in dict.fromkeys((toolchain, 'stable')):
        libdir = Path(capture(['rustup', 'run', channel, 'rustc', '--print', 'target-libdir']))
        candidate = libdir.parent / 'bin' / name
        if candidate.is_file():
            return str(candidate)
    raise ValueError('LLVM tools missing; install rustup component llvm-tools-preview')


def source_hashes():
    paths = set()
    for directory in ('firmware/src', 'crates/nocfree-input/src'):
        paths.update((ROOT / directory).rglob('*.rs'))
    paths.update(ROOT / name for name in (
        'rust-toolchain.toml', 'firmware/Cargo.toml', 'firmware/Cargo.lock',
        'firmware/build.rs', 'firmware/keyboard.toml', 'firmware/vial.json',
        'firmware/.cargo/config.toml',
        'crates/nocfree-input/Cargo.toml'))
    paths.update((ROOT / 'firmware').glob('*.x'))
    return {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        'Outputs are candidates only: no flashing, release approval, or hardware validation. '
        'Run the host harness before building. Existing candidate files are replaced.'))
    parser.add_argument('--output', '-o', type=Path,
                        default=ROOT / '.evidence/main-firmware-update/candidates')
    parser.add_argument('--toolchain', '-t', default=tomllib.loads(
        (ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel'])
    parser.add_argument('--dry-run', '-n', action='store_true', help='Print the build plan only')
    parser.add_argument('--version', '-v', action='version', version='%(prog)s 1.0.0')
    args = parser.parse_args()
    try:
        manifest = tomllib.loads((ROOT / 'firmware/Cargo.toml').read_text())
        rmk = manifest['dependencies']['rmk']['rev']
        before = source_hashes()
        output = args.output.resolve()
        plan = {'rmk_revision': rmk, 'roles': {role: features(role) for role in ROLES},
                'output': str(output), 'toolchain': args.toolchain}
        if args.dry_run:
            print(json.dumps(plan, indent=2))
            return
        output.mkdir(parents=True, exist_ok=True)
        (output / 'manifest.json').unlink(missing_ok=True)
        objcopy = llvm_tool('llvm-objcopy', args.toolchain)
        size_tool = llvm_tool('llvm-size', args.toolchain)
        environment = dict(os.environ, CARGO_TARGET_DIR=str(output / 'target'))
        report = {'schema': 1, **plan, 'source_sha256': before,
                  'compiler': capture(['rustup', 'run', args.toolchain, 'rustc', '--version']),
                  'llvm_objcopy': capture([objcopy, '--version']), 'images': {},
                  'status': 'structural candidates only; hardware and release acceptance pending'}
        for role in ROLES:
            folder = output / role
            folder.mkdir(exist_ok=True)
            print(f'Building {role} with pinned RMK {rmk}...', flush=True)
            with (folder / 'build.log').open('w') as log:
                subprocess.run(['rustup', 'run', args.toolchain, 'cargo', 'build', '--locked',
                                '--release', '--bin', 'nocfree-rmk', '--target', TARGET,
                                '--no-default-features', '--features', ','.join(features(role))],
                               cwd=ROOT / 'firmware', env=environment, stdout=log, stderr=log,
                               check=True)
            elf = output / 'target' / TARGET / 'release/nocfree-rmk'
            subprocess.run([objcopy, '-O', 'binary', str(elf), str(folder / 'candidate.bin')],
                           check=True)
            (folder / 'size.txt').write_text(capture([size_tool, '-A', str(elf)]) + '\n')
            binary = (folder / 'candidate.bin').read_bytes()
            version = manifest['package']['version']
            if f'NocFree RMK;fw={version}'.encode() not in binary:
                raise ValueError(f'{role}: expected firmware version descriptor missing')
            image = encode(binary, 0x27000 if role == 'receiver' else 0x1000)
            result = guard(role, image, binary)
            (folder / 'candidate.uf2').write_bytes(image)
            (folder / 'guard.json').write_text(json.dumps(result, indent=2) + '\n')
            report['images'][role] = {'features': features(role), 'guard': result,
                                     'binary_sha256': digest(binary), 'uf2_sha256': digest(image),
                                     'binary_size': len(binary)}
            print(f'{role}: image checks passed ({len(binary)} bytes)', flush=True)
        if source_hashes() != before:
            raise ValueError('firmware sources changed during build; rebuild candidates')
        (output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f'Candidate manifest: {output / "manifest.json"}')
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'Build rejected: {error}\n')


if __name__ == '__main__':
    main()
