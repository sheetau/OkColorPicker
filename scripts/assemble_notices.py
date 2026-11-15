"""Collect reviewed notices and locked Rust sources without exposing Cargo paths."""
from argparse import ArgumentParser
from hashlib import sha256
from html import escape
from pathlib import Path
import json
import re
import subprocess
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
ABOUT_VERSION = 'cargo-about 0.9.3'
VOID_README_SHA = 'f85783a6fcf9ecc19edabd710775a88430d9e886f46728bfd7d65cef55ff3e73'
TERMINAL_LICENSE_SHA = '28a7dfa51e1b3895b43462fe1c07fcc56e687b09fdfe91537f0e09b888573e07'
GENERIC_MIT_SHA = 'b05785f9f18e6716bab63424b11454513b9943a222595b70411009202fc592b5'


def run(*command, cwd=ROOT):
    return subprocess.check_output(command, cwd=cwd)


def checked_bytes(path, expected):
    data = path.read_bytes()
    if sha256(data).hexdigest() != expected:
        raise ValueError(f'Source hash mismatch: {path.name}')
    return data


def safe_name(value):
    if not re.fullmatch(r'[A-Za-z0-9_.+-]+', value) or value in {'.', '..'}:
        raise ValueError('Unsafe archive name')
    return value


def notice_sections(report, root=ROOT):
    """Keep original texts, using hash-pinned snapshots for absent upstream files."""
    snapshots = {}
    provenance = root / 'licenses/clarifications/sources.json'
    if provenance.is_file():
        config = tomllib.loads((root / 'licenses/about.toml').read_text(encoding='utf-8'))
        for record in json.loads(provenance.read_text(encoding='utf-8')):
            identity = (record['name'], record['version'])
            approved = config.get(record['name'], {}).get('clarify', {}).get('git', [])
            if identity in snapshots or not any(entry['license'] == 'MIT' and entry['checksum'] == record['sha256'] for entry in approved):
                raise ValueError('Unreviewed or duplicate license snapshot')
            text = checked_bytes(provenance.parent / safe_name(record['file']), record['sha256']).decode('utf-8')
            snapshots[identity] = (text, f'Unchanged upstream notice: {record["url"]}')
    sections = []
    for entry in report['licenses']:
        users = entry['used_by']
        if entry['id'] != 'MIT' or entry.get('source_path'):
            sections.append((entry['name'], [u['crate'] for u in users], entry['text'], ''))
            continue
        if sha256(entry['text'].encode('utf-8')).hexdigest() != GENERIC_MIT_SHA:
            raise ValueError('Unreviewed generic MIT text')
        for user in users:
            package = user['crate']
            identity = (package['name'], package['version'])
            if identity in snapshots:
                text, origin = snapshots[identity]
            elif identity == ('zgui-color', '0.1.0'):
                text = checked_bytes(root / 'vendor/zgui-wgsl/LICENSE-MIT', TERMINAL_LICENSE_SHA).decode('utf-8')
                origin = ('The MIT-covered gamma table is derived from Microsoft Windows Terminal. '
                          'Its original notice is reproduced below; zgui NOTICE identifies the derived file.')
            elif identity == ('void', '1.0.2') and package['license'] == 'MIT':
                readme = Path(package['manifest_path']).parent / 'README.md'
                original = checked_bytes(readme, VOID_README_SHA).decode('utf-8')
                # Upstream declares MIT but supplies no full license file. Preserve its
                # actual author/license notice, then supply the standard permission terms.
                # Do not turn an author credit into an invented copyright assertion.
                terms = entry['text'].replace('Copyright (c) <year> <copyright holders>\n\n', '', 1)
                text = original + '\n\nStandard MIT permission terms\n\n' + terms
                origin = ('README from rust-void tag 1.0.2, commit ab2f4dbb7c95c144ccba2fa8afd11e155aa91133. '
                          'Upstream declares MIT and credits Jonathan Reem, but provides no full license file. '
                          'The standard MIT terms follow that unchanged README; no copyright year or holder is inferred.')
            else:
                raise ValueError(f'Unreviewed generic MIT notice: {identity[0]} {identity[1]}')
            sections.append((entry['name'], [package], text, origin))
    return sections


def supplemental_sections(root=ROOT):
    paths = [root / 'LICENSE-APACHE', root / 'NOTICE', root / 'licenses/zgui-NOTICE',
             root / 'licenses/MartianMono-OFL.txt', root / 'licenses/clipremote-MIT.txt']
    dxc = root / 'licenses/dxc'
    provenance = json.loads((dxc / 'sources.json').read_text(encoding='utf-8'))
    for record in provenance['files']:
        path = dxc / safe_name(record['file'])
        checked_bytes(path, record['sha256'])
        paths.append(path)
    for patch in sorted((root / 'vendor').iterdir()):
        for name in ('LICENSE', 'LICENSE-MIT', 'NOTICE'):
            path = patch / name
            if path.is_file():
                paths.append(path)
    return [(p.relative_to(root).as_posix(), [], p.read_text(encoding='utf-8'), '') for p in paths]


def render_html(sections):
    parts = ['<!doctype html><html lang="en"><meta charset="utf-8">',
             '<meta name="viewport" content="width=device-width,initial-scale=1">',
             '<title>OkColorPicker third-party notices</title>',
             '<style>body{max-width:72rem;margin:3rem auto;padding:0 1rem;font:16px/1.5 system-ui}'
             'pre{white-space:pre-wrap;overflow-wrap:anywhere;font:13px/1.5 monospace}'
             'section{margin:3rem 0;border-top:1px solid #aaa}li{overflow-wrap:anywhere}</style>',
             '<h1>OkColorPicker third-party notices</h1>',
             '<p>Application and Rust dependency sources are supplied in <a href="SOURCES.zip">SOURCES.zip</a>. '
             '<a href="SOURCE_INDEX.json">SOURCE_INDEX.json</a> maps each package to its source archive and checksum. '
             'Registry .crate files are gzip-compressed tar archives; Git snapshots are tar archives. '
             'MPL-covered source remains under MPL-2.0, including modifications. '
             'License texts below and source-file notices retain their original terms.</p>',
             '<p>Build dependencies are included. Native DXC notices are reproduced separately below; '
             'its pinned source locations are recorded in <a href="DXC_SOURCES.json">DXC_SOURCES.json</a>. '
             'Martian Grotesk is not included.</p>']
    for title, packages, text, origin in sections:
        parts.append('<section><h2>' + escape(title) + '</h2>')
        if packages:
            parts.append('<ul>' + ''.join('<li>' + escape(p['name'] + ' ' + p['version']) + '</li>'
                                         for p in sorted(packages, key=lambda p: (p['name'], p['version']))) + '</ul>')
        if origin:
            parts.append('<p>' + escape(origin) + '</p>')
        parts.append('<pre>' + escape(text) + '</pre></section>')
    return '\n'.join(parts) + '\n</html>\n'


def write_zip_entry(archive, name, data):
    # Fixed metadata makes identical source collections byte-for-byte reproducible.
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    archive.writestr(info, data)


def assemble_sources(packages, destination, commit, root=ROOT):
    lock = tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf-8'))
    checksums = {(p['name'], p['version'], p.get('source')): p.get('checksum') for p in lock['package']}
    records = []
    archived = {}
    with zipfile.ZipFile(destination, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for p in sorted(packages, key=lambda p: (p['name'], p['version'])):
            name, version = safe_name(p['name']), safe_name(p['version'])
            source = p['source']
            package_root = Path(p['manifest_path']).parent.resolve()
            if source and source.startswith('registry+'):
                expected = checksums.get((name, version, source))
                if not expected:
                    raise ValueError(f'No locked source checksum for {name} {version}')
                entry = f'crates/{name}-{version}.crate'
                cache = package_root.parents[2] / 'cache' / package_root.parent.name / f'{name}-{version}.crate'
                data = checked_bytes(cache, expected)
                origin = source
            elif source and source.startswith('git+'):
                revision = source.rsplit('#', 1)[-1]
                if not re.fullmatch('[0-9a-f]{40}', revision):
                    raise ValueError('Git dependency requires a full locked revision')
                entry = 'git/' + revision + '.tar'
                origin = source
                if run('git', 'status', '--porcelain', '--untracked-files=no', cwd=package_root).strip():
                    raise ValueError(f'Modified dependency checkout: {name}')
                if run('git', 'rev-parse', 'HEAD', cwd=package_root).decode().strip() != revision:
                    raise ValueError(f'Dependency checkout differs from locked revision: {name}')
                if entry not in archived:
                    top = Path(run('git', 'rev-parse', '--show-toplevel', cwd=package_root).decode().strip())
                    data = run('git', 'archive', '--format=tar', revision, cwd=top)
            elif source is None and package_root.is_relative_to(root):
                relative_manifest = (package_root / 'Cargo.toml').relative_to(root).as_posix()
                run('git', 'ls-files', '--error-unmatch', relative_manifest, cwd=root)
                entry = 'project/' + commit + '.tar'
                origin = 'project:' + commit
                if entry not in archived:
                    data = run('git', 'archive', '--format=tar', commit, cwd=root)
            else:
                raise ValueError(f'Unsupported source for {name} {version}')
            if entry not in archived:
                write_zip_entry(archive, entry, data)
                archived[entry] = sha256(data).hexdigest()
            elif source and source.startswith('registry+') and archived[entry] != sha256(data).hexdigest():
                raise ValueError(f'Conflicting source archives for {name} {version}')
            records.append({'name': name, 'version': version, 'license': p['license'],
                            'source': origin, 'archive': entry, 'sha256': archived[entry]})
    return records


def assemble(output, cargo_about):
    output = output.resolve()
    if output.exists():
        raise ValueError('Output already exists; choose a new directory')
    if run(cargo_about, '--version').decode().strip() != ABOUT_VERSION:
        raise ValueError(f'{ABOUT_VERSION} is required')
    if run('git', 'status', '--porcelain').strip():
        raise ValueError('Commit source changes before assembling source snapshots')
    commit = run('git', 'rev-parse', 'HEAD').decode().strip()
    lock_hash = sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest()
    output.parent.mkdir(parents=True, exist_ok=True)
    # cargo-about rejects redirected stdout under a PowerShell ancestor, even
    # when Python reads the bytes directly. Use its explicit UTF-8 file output.
    with tempfile.TemporaryDirectory(prefix='.license-report-', dir=output.parent) as directory:
        report_root = Path(directory).resolve()
        if not report_root.is_relative_to(output.parent):
            raise ValueError('Report directory escaped output parent')
        report_path = report_root / 'dependencies.json'
        run(cargo_about, 'generate', '--locked', '--offline', '--fail', '--manifest-path',
            'crates/okcolor-app/Cargo.toml', '--config', 'licenses/about.toml', '--format', 'json',
            '--output-file', str(report_path))
        report = json.loads(report_path.read_text(encoding='utf-8'))
    packages = {u['crate']['id']: u['crate'] for e in report['licenses'] for u in e['used_by']}
    declared = {p['package']['id'] for p in report['crates']}
    if declared != set(packages):
        raise ValueError('License report does not cover every resolved package')
    html = render_html(supplemental_sections() + notice_sections(report))
    with tempfile.TemporaryDirectory(prefix='.notices-', dir=output.parent) as directory:
        stage = Path(directory).resolve()
        if not stage.is_relative_to(output.parent):
            raise ValueError('Staging directory escaped output parent')
        records = assemble_sources(packages.values(), stage / 'SOURCES.zip', commit)
        files = {'THIRD_PARTY_NOTICES.html': html,
                 'SOURCE_INDEX.json': json.dumps(records, ensure_ascii=False, indent=2) + '\n',
                 'DXC_SOURCES.json': (ROOT / 'licenses/dxc/sources.json').read_text(encoding='utf-8')}
        for name, text in files.items():
            for private in (str(ROOT), str(ROOT).replace('\\', '/'), str(Path.home()), str(Path.home()).replace('\\', '/')):
                if private.lower() in text.lower() or escape(private).lower() in text.lower():
                    raise ValueError('Machine-specific path in distribution metadata')
            (stage / name).write_text(text, encoding='utf-8', newline='\n')
        if run('git', 'rev-parse', 'HEAD').decode().strip() != commit or run('git', 'status', '--porcelain').strip():
            raise ValueError('Source changed during assembly')
        if sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest() != lock_hash:
            raise ValueError('Lockfile changed during assembly')
        manifest = {'project_commit': commit, 'cargo_lock_sha256': lock_hash, 'cargo_about': ABOUT_VERSION,
                    'target': 'x86_64-pc-windows-msvc', 'package_count': len(packages),
                    'files': {p.name: sha256(p.read_bytes()).hexdigest() for p in sorted(stage.iterdir())}}
        (stage / 'BUNDLE.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8', newline='\n')
        stage.rename(output)
    print(f'Created {output.name}: notices and source for {len(packages)} packages')


if __name__ == '__main__':
    parser = ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cargo-about', default='cargo-about', help='Path to cargo-about 0.9.3')
    args = parser.parse_args()
    assemble(args.output, args.cargo_about)
