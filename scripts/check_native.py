#!/usr/bin/env python3
"""Cross-build untouched rmk-boot and compare its application memory maps."""
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
REVISION = '9ac30d13c50912c9c866f1f68c688e8234f836b8'


def check():
    # Only build; upstream Cargo runner settings are never invoked.
    with tempfile.TemporaryDirectory(prefix='nocfree-rmk-boot-') as temporary:
        source = pathlib.Path(temporary)
        subprocess.run(['git', 'clone', '--quiet', 'https://github.com/rmk-rs/rmk-boot', temporary], check=True)
        subprocess.run(['git', 'checkout', '--quiet', REVISION], cwd=source, check=True)
        subprocess.run(['rustup', 'target', 'add', 'thumbv7em-none-eabihf'], cwd=source, check=True)
        for profile, features, filename in (
            ('dual', 'nrf52833', 'rmk-boot-nrf52833-memory.x'),
            ('noswap', 'nrf52833,noswap', 'rmk-boot-nrf52833-noswap-memory.x'),
        ):
            subprocess.run(['cargo', 'build', '--locked', '--release', '--target', 'thumbv7em-none-eabihf',
                            '--features', features], cwd=source, check=True)
            expected = ROOT / 'firmware' / f'memory-rmk-{profile}.x'
            if (source / filename).read_bytes() != expected.read_bytes():
                raise SystemExit(f'{profile}: upstream memory layout changed')
            print(f'{profile}: build and memory map match {REVISION}')
    print('Compile proof only. These boot images are not approved for NocFree installation.')


if __name__ == '__main__':
    check()
