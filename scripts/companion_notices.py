#!/usr/bin/env python3
"""Collect locked dependency license notices for the macOS application bundle."""
import json
from pathlib import Path
import subprocess
import sys

metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', 'aarch64-apple-darwin'], cwd=Path(__file__).resolve().parents[1] / 'desktop'))
nodes = {n['id']: n for n in metadata['resolve']['nodes']}
pending = [metadata['resolve']['root']]
selected = set()
while pending:
    identity = pending.pop()
    if identity in selected:
        continue
    selected.add(identity)
    pending.extend(d['pkg'] for d in nodes[identity]['deps'])
parts = ['# Third-party notices\n\nLocked desktop dependencies used by NocFree RMK Companion. This inventory excludes bundled firmware; its pinned RMK, Embassy and other firmware notices require a separate audit before publication.\n']
for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
    if package['id'] not in selected or package['name'] == 'nocfree-companion':
        continue
    root = Path(package['manifest_path']).parent
    parts.append(f"\n## {package['name']} {package['version']}\n\nLicense: {package.get('license') or 'See license file'}\n\n")
    files = set(p for p in root.iterdir() if p.is_file() and p.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE')))
    if package.get('license_file'):
        files.add(root / package['license_file'])
    if not files:
        parts.append('License text absent from the packaged crate; upstream notice review remains required.\n')
    if '--inventory' in sys.argv:
        continue
    for path in sorted(files):
        parts.append(f"### {path.name}\n\n```text\n{path.read_text(errors='replace')}\n```\n")
Path(sys.argv[1]).write_text(''.join(parts))
