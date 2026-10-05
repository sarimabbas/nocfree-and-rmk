#!/usr/bin/env python3
"""Render audited, hash-pinned desktop and firmware notices without network access."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
HASH = re.compile(r"[0-9a-f]{64}\Z")
SUPPLEMENTARY_TERMS = {
    'CC0-1.0': 'a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499',
    'MIT': 'b05785f9f18e6716bab63424b11454513b9943a222595b70411009202fc592b5',
    'Apache-2.0': '074e6e32c86a4c0ef8b3ed25b721ca23aca83df277cd88106ef7177c354615ff',
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def release_pins(root):
    """Read the public pin allowlist, without importing private release evidence tools."""
    tree = ast.parse((root / 'scripts/package_companion_release.py').read_text())
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'ROLES' for t in node.targets):
            roles = ast.literal_eval(node.value)
            return {role: {'source_commit': spec[3], 'rmk_revision': spec[4]} for role, spec in roles.items()}
    raise ValueError('Release firmware pin allowlist is missing')


def load_inventory(root):
    folder = root / 'docs/notices'
    inventory = json.loads((folder / 'inventory.json').read_text())
    if inventory.get('schema') != 1:
        raise ValueError('Unsupported notices inventory schema')
    desktop = inventory['desktop']
    for filename, key in [('Cargo.lock', 'cargo_lock_sha256'), ('Cargo.toml', 'cargo_manifest_sha256')]:
        if digest((root / 'desktop' / filename).read_bytes()) != desktop[key]:
            raise ValueError(f'Desktop {filename} changed; recapture and review the notices inventory')
    firmware = inventory['firmware']
    if firmware['roles'] != release_pins(root):
        raise ValueError('Release firmware pins changed; recapture and review the notices inventory')
    graphs = {g['source_commit']: g for g in firmware['graphs']}
    if len(graphs) != len(firmware['graphs']) or set(graphs) != {p['source_commit'] for p in firmware['roles'].values()}:
        raise ValueError('Firmware inventory graphs do not match the release pins')
    for pin in firmware['roles'].values():
        if graphs[pin['source_commit']]['rmk_revision'] != pin['rmk_revision']:
            raise ValueError('Firmware RMK revision does not match its release pin')
    texts = {}
    for group in [desktop, *firmware['graphs']]:
        identities = set()
        for package in group['packages']:
            identity = (package['name'], package['version'], package['source'])
            if identity in identities:
                raise ValueError('Duplicate package in notices inventory')
            identities.add(identity)
            if not package['texts'] and not package.get('unresolved'):
                raise ValueError(f'{package["name"]}: missing texts must have an explicit unresolved reason')
            for notice in package['texts']:
                sha = notice['sha256']
                if not HASH.fullmatch(sha) or notice['file'] != f'texts/{sha}.txt':
                    raise ValueError('Notice filename must match its SHA-256')
                data = (folder / notice['file']).read_bytes()
                if digest(data) != sha:
                    raise ValueError(f'Notice text checksum mismatch: {sha}')
                texts[sha] = data.decode('utf-8').replace('\r\n', '\n').replace('\r', '\n')
            if evidence := package.get('license_evidence'):
                selected = evidence['selected_license']
                if selected not in SUPPLEMENTARY_TERMS or evidence['terms']['sha256'] != SUPPLEMENTARY_TERMS[selected]:
                    raise ValueError('Supplementary license terms must match the audited SPDX text')
                if not evidence.get('upstream_omission') or not package.get('published_archive_sha256'):
                    raise ValueError('Supplementary terms require explicit upstream omission and archive provenance')
                if any(evidence[key] not in package['texts'] for key in ['declaration', 'terms']):
                    raise ValueError('Supplementary terms require preserved declaration and license text')
                declaration = tomllib.loads(texts[evidence['declaration']['sha256']])['package']['license']
                choices = re.split(r'\s+OR\s+|\s*/\s*', declaration)
                if selected not in choices or declaration != package['license']:
                    raise ValueError('Selected supplementary license must be declared by the published package')
    return inventory, texts


def render(inventory, texts, inventory_only=False):
    parts = ['# Third-party notices\n\nNocFree RMK Companion and its pinned bundled RMK firmware. License declarations below are upstream package metadata; full source notice texts are reproduced in the appendix. This conservative resolved dependency inventory includes build tools and optional firmware dependencies, and does not claim that every listed crate is linked into every image. Project-owned code is covered by the repository LICENSE and NOTICE.md. Factory firmware and factory bootloaders are not distributed.\n']
    groups = [('Desktop', inventory['desktop'])]
    for graph in inventory['firmware']['graphs']:
        roles = sorted(role.replace('receiver', 'dongle') for role, pin in inventory['firmware']['roles'].items() if pin['source_commit'] == graph['source_commit'])
        groups.append(('Firmware: ' + ', '.join(roles), graph))
    gaps = set()
    for title, group in groups:
        parts.append(f'\n## {title}\n\n')
        if 'source_commit' in group:
            parts.append(f'Source commit: `{group["source_commit"]}`. RMK revision: `{group["rmk_revision"]}`. Target: `{inventory["firmware"]["target"]}`. Features: {inventory["firmware"]["features"]}.\n\n')
        else:
            parts.append(f'Target: `{group["target"]}`.\n\n')
        parts.append(f'Cargo.lock SHA-256: `{group["cargo_lock_sha256"]}`.\n')
        for package in sorted(group['packages'], key=lambda p: (p['name'], p['version'], p['source'])):
            parts.append(f'\n### {package["name"]} {package["version"]}\n\nDeclared license: {package["license"]}\n\nSource: `{package["source"]}`\n\n')
            if package.get('notice_context'):
                parts.append(package['notice_context'] + '\n\n')
            if evidence := package.get('license_evidence'):
                parts.append(f'Selected license: {evidence["selected_license"]}. Upstream omission: {evidence["upstream_omission"]}.\n\n')
            for notice in package['texts']:
                link = f'../docs/notices/texts/{notice["sha256"]}.txt' if inventory_only else f'#notice-{notice["sha256"]}'
                parts.append(f'- {notice["origin"]} — [full text]({link})\n')
            if package.get('unresolved'):
                gap = f'{package["name"]} {package["version"]}: {package["unresolved"]}'
                gaps.add(gap)
                parts.append(f'Unresolved source-text gap: {package["unresolved"]}.\n')
    if gaps:
        parts.append('\n## Unresolved source-text gaps\n\n')
        parts.extend(f'- {gap}\n' for gap in sorted(gaps))
    if not inventory_only:
        parts.append('\n## Full source notice texts\n')
        for sha, value in sorted(texts.items()):
            parts.append(f'\n<a id="notice-{sha}"></a>\n\n### Notice {sha}\n\n')
            # A license may itself contain Markdown fences; use a longer delimiter.
            fence = '`' * max(3, 1 + max((len(m.group()) for m in re.finditer(r'`+', value)), default=0))
            parts.append(f'{fence}text\n{value.rstrip()}\n{fence}\n')
    return ''.join(parts), sorted(gaps)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', nargs='?', type=Path, default=ROOT / 'desktop/THIRD_PARTY_NOTICES.md', help='Output Markdown path (default: desktop/THIRD_PARTY_NOTICES.md)')
    parser.add_argument('-i', '--inventory', action='store_true', help='Omit the full text appendix')
    parser.add_argument('-c', '--check', action='store_true', help='Check output is current without modifying it')
    parser.add_argument('-s', '--strict', action='store_true', help='Fail if any source notice text remains unresolved (publication check)')
    parser.add_argument('-o', '--output', dest='output_option', type=Path, help='Output path (alternative to the existing positional path)')
    args = parser.parse_args(argv)
    if args.output_option is not None:
        args.output = args.output_option
    try:
        inventory, texts = load_inventory(ROOT)
        output, gaps = render(inventory, texts, args.inventory)
        if args.strict and gaps:
            raise ValueError('Unresolved source notice texts:\n' + '\n'.join(gaps))
        if args.check:
            if not args.output.exists() or args.output.read_text() != output:
                raise ValueError('Notices output is out of date; regenerate it')
        else:
            args.output.write_text(output)
        print(f'{len(texts)} full notice texts; {len(gaps)} unresolved source-text gaps', file=sys.stderr)
        return 0
    except (ValueError, KeyError, OSError, UnicodeError) as error:
        print(f'Notices: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
