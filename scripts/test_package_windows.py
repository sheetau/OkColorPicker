"""Checks that packaging rejects wrong binaries and damaged/source-mismatched bundles."""
from pathlib import Path
import json
import struct
import tempfile
import unittest
from hashlib import sha256

import package_windows as package


class PackageTests(unittest.TestCase):
    def test_only_x64_gui_executables_are_accepted(self):
        data = bytearray(512)
        data[:2] = b'MZ'
        struct.pack_into('<I', data, 0x3c, 64)
        data[64:68] = b'PE\0\0'
        struct.pack_into('<HH', data, 68, 0x8664, 0)
        struct.pack_into('<H', data, 88, 0x20b)
        struct.pack_into('<H', data, 156, 2)
        self.assertEqual(package.inspect_pe(data)['subsystem'], 'Windows GUI')
        struct.pack_into('<H', data, 156, 3)
        with self.assertRaisesRegex(ValueError, 'GUI subsystem'):
            package.inspect_pe(data)
        struct.pack_into('<H', data, 156, 2)
        struct.pack_into('<H', data, 68, 0xaa64)
        with self.assertRaisesRegex(ValueError, 'x64'):
            package.inspect_pe(data)

    def test_bundle_identity_checksums_and_file_allowlist(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            names = ['THIRD_PARTY_NOTICES.html', 'SOURCES.zip', 'SOURCE_INDEX.json', 'DXC_SOURCES.json']
            for name in names:
                (root / name).write_bytes(b'fixture content')
            manifest = {'project_commit': 'expected', 'files': {n: sha256((root / n).read_bytes()).hexdigest() for n in names}}
            (root / 'BUNDLE.json').write_text(json.dumps(manifest), encoding='utf-8')
            package.verify_notice_bundle(root, 'expected')
            with self.assertRaisesRegex(ValueError, 'different project commits'):
                package.verify_notice_bundle(root, 'different')
            (root / 'private.sqlite').write_bytes(b'private')
            with self.assertRaisesRegex(ValueError, 'Unexpected or missing'):
                package.verify_notice_bundle(root, 'expected')
            (root / 'private.sqlite').unlink()
            (root / 'SOURCES.zip').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'Source hash mismatch'):
                package.verify_notice_bundle(root, 'expected')


if __name__ == '__main__':
    unittest.main()
