#!/usr/bin/env python3
"""Package the native Windows or Linux build with its validated firmware and notices."""
import argparse
import hashlib
import json
import platform
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', action='version', version='NocFree portable packaging 1')
    parser.parse_args()
    system = {'Windows': 'windows', 'Linux': 'linux'}.get(platform.system())
    if system is None or platform.machine().lower() not in ('x86_64', 'amd64'):
        parser.error('Run this script on Windows or Linux x86-64.')
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--manifest-path', str(ROOT / 'desktop/Cargo.toml'),
        '--locked', '--no-deps', '--format-version', '1']))
    version = next(package['version'] for package in metadata['packages']
                   if package['name'] == 'nocfree-companion')
    subprocess.run(['cargo', 'build', '--manifest-path', str(ROOT / 'desktop/Cargo.toml'),
                    '--locked', '--release'], check=True)
    executable = ROOT / 'desktop/target/release' / ('nocfree-companion.exe' if system == 'windows' else 'nocfree-companion')
    firmware = ROOT / 'dist/companion-firmware'
    subprocess.run([str(executable), '--check-firmware', str(firmware)], check=True)
    name = f'nocfree-rmk-companion-{version}-{system}-x86_64'
    bundle = ROOT / 'dist' / name
    if bundle.exists():
        shutil.rmtree(bundle)
    resources = bundle / 'Resources'
    resources.mkdir(parents=True)
    shutil.copy2(executable, bundle / executable.name)
    (resources / 'Firmware').mkdir()
    for firmware_name in ('manifest.json', 'left.uf2', 'left.bin', 'right.uf2', 'right.bin', 'receiver.uf2', 'receiver.bin'):
        shutil.copy2(firmware / firmware_name, resources / 'Firmware' / firmware_name)
    for source, target in [('LICENSE', 'LICENSE.txt'), ('NOTICE.md', 'NOTICE.md')]:
        shutil.copy2(ROOT / source, resources / target)
    subprocess.run(['python3' if system == 'linux' else 'python',
                    str(ROOT / 'scripts/companion_notices.py'),
                    str(resources / 'THIRD_PARTY_NOTICES.md')], check=True)
    instructions = 'Extract all files. Keep Resources beside the app.\n\n'
    if system == 'linux':
        shutil.copy2(ROOT / 'desktop/70-nocfree.rules', bundle / '70-nocfree.rules')
        instructions += ('Run ./nocfree-companion from a desktop session.\n'
                         'Requires Vulkan, fontconfig, X11 or Wayland, and zip for log export.\n'
                         'On Ubuntu 24.04 install: libvulkan1 libfontconfig1 libxkbcommon0 '
                         'libxkbcommon-x11-0 libxcb-xkb1 libxcb-render0 libxcb-shape0 '
                         'libxcb-xfixes0 libxcb1 libudev1 bluez zip.\n'
                         'For USB access, an administrator can copy 70-nocfree.rules to '
                         '/etc/udev/rules.d/, run udevadm control --reload-rules, then reconnect the keyboard.\n'
                         'Mount the recovery drive in the file manager when asked.\n')
    else:
        instructions += ('Open nocfree-companion.exe.\n'
                         'Automatic recovery can require a WinUSB driver for the DFU interface.\n'
                         'Use the keyboard recovery shortcut if automatic recovery is unavailable.\n'
                         'Do not replace the keyboard HID or recovery-drive drivers.\n')
    (bundle / 'README.txt').write_text(instructions, encoding='utf-8')
    archive = Path(shutil.make_archive(str(ROOT / 'dist' / name),
                                      'zip' if system == 'windows' else 'gztar',
                                      root_dir=bundle.parent, base_dir=bundle.name))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + '.sha256').write_text(f'{digest}  {archive.name}\n', encoding='ascii')
    print(f'Ready: {archive.name}')


if __name__ == '__main__':
    main()
