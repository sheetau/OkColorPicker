"""Distribution privacy, source-integrity and notice rendering contracts."""
from pathlib import Path
from hashlib import sha256
import io
import subprocess
import tarfile
import tempfile
import unittest
import zipfile

import assemble_notices as assembly


class NoticesTests(unittest.TestCase):
    def test_license_text_and_attribution_are_html_escaped_without_metadata_leaks(self):
        report = {'licenses': [{'id': 'MIT', 'name': 'MIT <test>', 'source_path': 'C:/private/LICENSE',
                               'text': '<script>alert("license")</script>', 'used_by': [{'crate': {
                                   'name': 'sample<name>', 'version': '1.0', 'manifest_path': 'C:/private/Cargo.toml',
                                   'id': 'path+C:/private', 'authors': ['private marker']}}]}]}
        html = assembly.render_html(assembly.notice_sections(report))
        self.assertIn('&lt;script&gt;', html)
        self.assertIn('sample&lt;name&gt;', html)
        self.assertNotIn('<script>', html)
        self.assertNotIn('C:/private', html)
        self.assertNotIn('private marker', html)

    def test_unknown_fallback_does_not_silently_become_a_distribution_notice(self):
        entry = {'id': 'MIT', 'name': 'MIT', 'source_path': None, 'text': 'unreviewed', 'used_by': []}
        with self.assertRaisesRegex(ValueError, 'Unreviewed generic MIT text'):
            assembly.notice_sections({'licenses': [entry]})

    def test_source_snapshot_excludes_ignored_private_data(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            (root / '.gitignore').write_text('private/\nSOURCES.zip\n', encoding='utf-8')
            (root / 'Cargo.toml').write_text('[package]\nname="test-app"\nversion="0.1.0"\n', encoding='utf-8')
            (root / 'Cargo.lock').write_text('version=4\npackage=[]\n', encoding='utf-8')
            (root / 'private').mkdir()
            (root / 'private/state.sqlite').write_bytes(b'owner-only-marker')
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            subprocess.run(['git', 'add', '.'], cwd=root, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                            'commit', '-qm', 'Source fixture'], cwd=root, check=True)
            commit = assembly.run('git', 'rev-parse', 'HEAD', cwd=root).decode().strip()
            packages = [{'name': 'test-app', 'version': '0.1.0', 'source': None, 'license': 'MIT',
                         'manifest_path': str(root / 'Cargo.toml')}]
            index = assembly.assemble_sources(packages, root / 'SOURCES.zip', commit, root)
            self.assertNotIn(str(root), str(index))
            with zipfile.ZipFile(root / 'SOURCES.zip') as archive:
                data = archive.read(index[0]['archive'])
                self.assertEqual(sha256(data).hexdigest(), index[0]['sha256'])
                with tarfile.open(fileobj=io.BytesIO(data)) as source:
                    self.assertIn('Cargo.toml', source.getnames())
                    self.assertNotIn('private/state.sqlite', source.getnames())
                self.assertNotIn(b'owner-only-marker', data)

    def test_registry_archive_must_match_the_lockfile(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            registry = root / 'registry'
            package = registry / 'src/mock-index/sample-1.0.0'
            package.mkdir(parents=True)
            cache = registry / 'cache/mock-index'
            cache.mkdir(parents=True)
            original = b'locked source bytes'
            (cache / 'sample-1.0.0.crate').write_bytes(original)
            source = 'registry+https://example.invalid/index'
            (root / 'Cargo.lock').write_text(
                'version=4\n[[package]]\nname="sample"\nversion="1.0.0"\n'
                f'source="{source}"\nchecksum="{sha256(original).hexdigest()}"\n', encoding='utf-8')
            packages = [{'name': 'sample', 'version': '1.0.0', 'source': source, 'license': 'MIT',
                         'manifest_path': str(package / 'Cargo.toml')}]
            assembly.assemble_sources(packages, root / 'good.zip', 'unused', root)
            (cache / 'sample-1.0.0.crate').write_bytes(b'modified source bytes')
            with self.assertRaisesRegex(ValueError, 'Source hash mismatch'):
                assembly.assemble_sources(packages, root / 'bad.zip', 'unused', root)

    def test_archive_names_cannot_escape_the_archive(self):
        for value in ['../private', '.', '..', 'C:\\private', 'path/file']:
            with self.assertRaises(ValueError):
                assembly.safe_name(value)


if __name__ == '__main__':
    unittest.main()
