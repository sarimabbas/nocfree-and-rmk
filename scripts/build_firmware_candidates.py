#!/usr/bin/env python3
"""Build firmware images and check their addresses, vectors and board family."""
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
import prepare_migration

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'thumbv7em-none-eabihf'
ROLES = ('left', 'right', 'receiver')
LAYOUTS = ('ansi', 'iso', 'jis', 'kr')


def features(role, layout='ansi'):
    if layout not in LAYOUTS:
        raise ValueError('Unknown keyboard layout')
    result = ['defmt-logging', role, 'runtime-recovery']
    if role != 'receiver':
        result.append('mac-keymap')
        if layout != 'ansi':
            result.append(f'layout-{layout}')
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
        return migration_guard.inspect_startup_image(image, binary, role)
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


def validate_dependency_sources(manifest):
    sources = ((manifest['dependencies']['rmk'], 'dependencies/rmk/rmk'),
               (manifest['dependencies']['rmk-types'], 'dependencies/rmk/rmk-types'),
               (manifest['patch']['crates-io']['embassy-nrf'], 'dependencies/embassy-nrf'))
    for source, expected in sources:
        if 'path' not in source or (ROOT / 'firmware' / source['path']).resolve() != (ROOT / expected).resolve():
            raise ValueError(f'dependency source must use the prepared {expected}')


def source_hashes():
    paths = set()
    for directory in ('firmware/src', 'crates/nocfree-input/src'):
        paths.update((ROOT / directory).rglob('*.rs'))
    paths.update(ROOT / name for name in (
        'rust-toolchain.toml', 'firmware/Cargo.toml', 'firmware/Cargo.lock',
        'firmware/build.rs', 'firmware/keyboard.toml', 'firmware/vial.json',
        'firmware/.cargo/config.toml',
        'crates/nocfree-input/Cargo.toml'))
    paths.update(ROOT / 'scripts' / name for name in (
        'prepare_migration.py', 'build_firmware_candidates.py',
        'image_guard.py', 'migration_guard.py'))
    paths.update((ROOT / 'firmware/patches').glob('*.patch'))
    paths.update((ROOT / 'firmware').glob('*.x'))
    paths.update((ROOT / 'firmware').glob('vial-*.json'))
    paths.update(path for path in (ROOT / 'firmware/layouts').rglob('*') if path.is_file())
    return {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        'Run scripts/check.sh before building. Existing output files are replaced.'))
    layouts = parser.add_mutually_exclusive_group()
    layouts.add_argument('--layout', choices=LAYOUTS, default='ansi',
                        help='Physical keyboard layout (default: ansi)')
    layouts.add_argument('--all-layouts', action='store_true',
                        help='Build each layout in its own output directory')
    parser.add_argument('--output', '-o', type=Path,
                        default=ROOT / 'dist/firmware')
    parser.add_argument('--toolchain', '-t', default=tomllib.loads(
        (ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel'])
    parser.add_argument('--dry-run', '-n', action='store_true', help='Print the build plan only')
    parser.add_argument('--version', '-v', action='version', version='%(prog)s 1.0.0')
    args = parser.parse_args()
    try:
        manifest = tomllib.loads((ROOT / 'firmware/Cargo.toml').read_text())
        validate_dependency_sources(manifest)
        rmk = prepare_migration.RMK_REVISION
        if not args.dry_run:
            prepare_migration.prepare()
        dependency_hash = (prepare_migration.source_digest(ROOT / "dependencies")
                           if not args.dry_run else None)
        before = source_hashes()
        for layout in LAYOUTS if args.all_layouts else (args.layout,):
            output = args.output.resolve() / layout if args.all_layouts else args.output.resolve()
            plan = {'layout': layout, 'rmk_revision': rmk, 'roles': {role: features(role, layout) for role in ROLES},
                    'output': str(output), 'toolchain': args.toolchain}
            if args.dry_run:
                print(json.dumps(plan, indent=2))
                continue
            output.mkdir(parents=True, exist_ok=True)
            (output / 'manifest.json').unlink(missing_ok=True)
            objcopy = llvm_tool('llvm-objcopy', args.toolchain)
            size_tool = llvm_tool('llvm-size', args.toolchain)
            target_dir = args.output.resolve() / 'target'
            environment = dict(os.environ, CARGO_TARGET_DIR=str(target_dir))
            report = {'schema': 1, **plan, 'source_sha256': before,
                      'compiler': capture(['rustup', 'run', args.toolchain, 'rustc', '--version']),
                      'llvm_objcopy': capture([objcopy, '--version']), 'images': {},
                      'status': 'image checks passed', 'hardware_validated': False,
                      'dependency_sha256': dependency_hash}
            for role in ROLES:
                folder = output / role
                folder.mkdir(exist_ok=True)
                print(f'Building {role} with pinned RMK {rmk}...', flush=True)
                with (folder / 'build.log').open('w') as log:
                    subprocess.run(['rustup', 'run', args.toolchain, 'cargo', 'build', '--locked',
                                    '--release', '--bin', 'nocfree-rmk', '--target', TARGET,
                                    '--no-default-features', '--features', ','.join(features(role, layout))],
                                   cwd=ROOT / 'firmware', env=environment, stdout=log, stderr=log,
                                   check=True)
                elf = target_dir / TARGET / 'release/nocfree-rmk'
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
                report['images'][role] = {'features': features(role, layout), 'guard': result,
                                         'binary_sha256': digest(binary), 'uf2_sha256': digest(image),
                                         'binary_size': len(binary)}
                print(f'{role}: image checks passed ({len(binary)} bytes)', flush=True)
            if prepare_migration.source_digest(ROOT / 'dependencies') != dependency_hash:
                raise ValueError('dependency sources changed during build; rebuild candidates')
            if source_hashes() != before:
                raise ValueError('firmware sources changed during build; rebuild candidates')
            (output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
            print(f'Candidate manifest: {output / "manifest.json"}')
    except (OSError, KeyError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'Build rejected: {error}\n')


if __name__ == '__main__':
    main()
