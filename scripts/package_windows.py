"""Build a local Windows x64 candidate with notices, source and file checksums."""
from argparse import ArgumentParser
from hashlib import sha256
from pathlib import Path
import json
import shutil
import struct
import tempfile
import tomllib
import zipfile

import assemble_notices as notices

ROOT = notices.ROOT
EXPECTED_CRT = {'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'}


def inspect_pe(data):
    """Read only the headers/import table of our built executable; never load it."""
    if data[:2] != b'MZ' or len(data) < 64:
        raise ValueError('Not a Windows executable')
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    if data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('Invalid PE signature')
    machine, section_count = struct.unpack_from('<HH', data, pe + 4)
    optional_size = struct.unpack_from('<H', data, pe + 20)[0]
    optional = pe + 24
    if machine != 0x8664 or struct.unpack_from('<H', data, optional)[0] != 0x20b:
        raise ValueError('The package requires an x64 PE32+ executable')
    subsystem = struct.unpack_from('<H', data, optional + 68)[0]
    if subsystem != 2:
        raise ValueError('The release executable must use the Windows GUI subsystem')
    sections = []
    for i in range(section_count):
        virtual_size, rva, size, offset = struct.unpack_from('<IIII', data, optional + optional_size + i * 40 + 8)
        sections.append((rva, min(virtual_size or size, size), offset))

    def raw(rva):
        for start, size, offset in sections:
            if start <= rva < start + size and offset + rva - start < len(data):
                return offset + rva - start
        raise ValueError('Import address outside file-backed PE sections')

    import_rva = struct.unpack_from('<I', data, optional + 120)[0]
    imports = set()
    if import_rva:
        descriptor = raw(import_rva)
        for _ in range(512):
            fields = struct.unpack_from('<IIIII', data, descriptor)
            if not any(fields):
                break
            name = raw(fields[3])
            end = data.index(b'\0', name, min(name + 256, len(data)))
            imports.add(data[name:end].decode('ascii').lower())
            descriptor += 20
        else:
            raise ValueError('Unterminated PE import table')
    return {'machine': 'AMD64', 'subsystem': 'Windows GUI', 'imports': sorted(imports)}


def verify_notice_bundle(folder, commit):
    manifest = json.loads((folder / 'BUNDLE.json').read_text(encoding='utf-8'))
    if manifest['project_commit'] != commit:
        raise ValueError('Binary and source bundle refer to different project commits')
    expected = {'THIRD_PARTY_NOTICES.html', 'SOURCES.zip', 'SOURCE_INDEX.json', 'DXC_SOURCES.json'}
    if set(manifest['files']) != expected or {p.name for p in folder.iterdir()} != expected | {'BUNDLE.json'}:
        raise ValueError('Unexpected or missing notice bundle files')
    for name, checksum in manifest['files'].items():
        notices.checked_bytes(folder / name, checksum)
    return manifest


def package(output, cargo_about):
    output = output.resolve()
    if output.exists():
        raise ValueError('Output already exists; choose a new directory')
    if notices.run('git', 'status', '--porcelain').strip():
        raise ValueError('Commit source changes before packaging')
    compiler = notices.run('rustc', '--version').decode().strip()
    if not compiler.startswith('rustc 1.98.0 '):
        raise ValueError('Rust 1.98.0 is required')
    commit = notices.run('git', 'rev-parse', 'HEAD').decode().strip()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']
    name = f'OkColorPicker-{notices.safe_name(version)}-windows-x64-{commit[:12]}'
    build = notices.run('cargo', 'build', '-p', 'okcolor-app', '--release', '--locked', '--message-format=json')
    artifacts = [row for line in build.splitlines() if (row := json.loads(line)).get('reason') == 'compiler-artifact'
                 and row['target']['name'] == 'okcolor-app' and row.get('executable') and not row['profile']['test']]
    if len(artifacts) != 1:
        raise ValueError('Cargo did not identify exactly one application executable')
    binary = Path(artifacts[0]['executable']).read_bytes()
    pe = inspect_pe(binary)
    if set(pe['imports']) & EXPECTED_CRT != EXPECTED_CRT:
        raise ValueError('CRT imports changed; review the packaged runtime instructions')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.package-', dir=output.parent) as directory:
        stage = Path(directory).resolve()
        if not stage.is_relative_to(output.parent):
            raise ValueError('Package staging escaped output parent')
        folder = stage / name
        notices.assemble(folder, cargo_about)
        bundle = verify_notice_bundle(folder, commit)
        (folder / 'OkColorPicker.exe').write_bytes(binary)
        shutil.copyfile(ROOT / 'packaging/START_HERE.txt', folder / 'START_HERE.txt')
        shutil.copyfile(ROOT / 'packaging/update.ps1', folder / 'update.ps1')
        metadata = {'version': version, 'project_commit': commit, 'candidate': True,
                    'rustc': compiler, 'target': bundle['target'], 'pe': pe,
                    'files': {p.name: sha256(p.read_bytes()).hexdigest() for p in sorted(folder.iterdir())}}
        (folder / 'PACKAGE.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8', newline='\n')
        archive_path = stage / (name + '.zip')
        with zipfile.ZipFile(archive_path, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for path in sorted(folder.iterdir()):
                notices.write_zip_entry(archive, name + '/' + path.name, path.read_bytes())
        with zipfile.ZipFile(archive_path) as archive:
            if archive.testzip() is not None:
                raise ValueError('Package ZIP failed its CRC check')
        (stage / 'SHA256SUMS.txt').write_text(sha256(archive_path.read_bytes()).hexdigest() + '  ' + archive_path.name + '\n',
                                           encoding='utf-8', newline='\n')
        if notices.run('git', 'rev-parse', 'HEAD').decode().strip() != commit or notices.run('git', 'status', '--porcelain').strip():
            raise ValueError('Source changed during packaging')
        stage.rename(output)
    print(f'Created candidate {name}.zip; no upload or publication performed')


if __name__ == '__main__':
    parser = ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cargo-about', default='cargo-about')
    args = parser.parse_args()
    package(args.output, args.cargo_about)
