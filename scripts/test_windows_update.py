"""Exercise the shipped Windows updater against an offline release and inert package files."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "packaging/update.ps1"
FILES = ("BUNDLE.json", "DXC_SOURCES.json", "SOURCE_INDEX.json", "SOURCES.zip",
         "THIRD_PARTY_NOTICES.html", "START_HERE.txt", "OkColorPicker.lua", "Painting-plugins.zip", "update.ps1", "OkColorPicker.exe")


@unittest.skipUnless(os.name == "nt", "requires Windows PowerShell and NTFS")
class UpdateTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name) / "Installed 日本語"
        self.root.mkdir()
        self.feed = Path(temp.name) / "feed.json"
        self.stage = Path(temp.name) / "staging"
        self.stage.mkdir()
        self.commit = "a" * 40
        self.folder = "OkColorPicker-0.2.0-windows-x64-" + self.commit[:12]
        self.archive = Path(temp.name) / (self.folder + ".zip")
        self.contents = {name: ("new " + name).encode() for name in FILES}
        self.contents["update.ps1"] = SCRIPT.read_bytes()
        self.contents["PACKAGE.json"] = json.dumps({
            "version": "0.2.0", "project_commit": self.commit, "target": "x86_64-pc-windows-msvc",
            "files": {name: hashlib.sha256(data).hexdigest() for name, data in self.contents.items()},
        }).encode()
        for name in self.contents:
            (self.root / name).write_bytes(("old " + name).encode())
        shutil.copyfile(SCRIPT, self.root / "update.ps1")
        (self.root / "user.txt").write_text("personal", encoding="utf-8")
        self.publish()

    def publish(self):
        with zipfile.ZipFile(self.archive, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, data in self.contents.items():
                archive.writestr(self.folder + "/" + name, data)
        release = {"tag_name": "v0.2.0", "draft": False, "prerelease": False, "assets": [{
            "name": self.archive.name, "state": "uploaded", "size": self.archive.stat().st_size,
            "digest": "sha256:" + hashlib.sha256(self.archive.read_bytes()).hexdigest(),
            "browser_download_url": "https://github.com/sheetau/OkColorPicker/releases/download/v0.2.0/" + self.archive.name,
        }]}
        self.feed.write_text(json.dumps(release), encoding="utf-8")

    def run_helper(self, choice="", command="Invoke-PackageUpdate", installed="0.1.0", offline=False):
        harness = r"""
. (Join-Path $env:FIXTURE_ROOT update.ps1)
$CurrentVersion = $env:FIXTURE_VERSION
$ParentId = 100
function Get-Process($Id,$Name) {
    if ($Id) {
        $proc = [pscustomobject]@{Id=$Id;Path=(Join-Path $env:FIXTURE_ROOT OkColorPicker.exe)}
        $proc | Add-Member ScriptMethod WaitForExit { param($timeout) $true }
        $proc | Add-Member ScriptMethod Dispose {}
        $proc
    }
}
function Invoke-RestMethod($Uri,$UserAgent,$TimeoutSec) {
    if ($env:FIXTURE_OFFLINE -eq '1') { throw 'Unexpected network request' }
    Get-Content -LiteralPath $env:FIXTURE_FEED -Raw -Encoding UTF8 | ConvertFrom-Json
}
function Invoke-WebRequest($Uri,[switch]$UseBasicParsing,$UserAgent,$TimeoutSec,$OutFile) {
    [IO.File]::Copy($env:FIXTURE_ARCHIVE,$OutFile)
}
function Start-Process($FilePath,$WorkingDirectory) {
    if ($FilePath -ine (Join-Path $env:FIXTURE_ROOT OkColorPicker.exe)) { throw 'Wrong relaunch path' }
    'RELAUNCH'
}
"""
        return subprocess.run(
            ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", harness + command],
            input=choice + "\n", capture_output=True, timeout=20, text=True, errors="replace",
            env={**os.environ, "FIXTURE_ROOT": str(self.root), "FIXTURE_VERSION": installed,
                 "FIXTURE_FEED": str(self.feed), "FIXTURE_ARCHIVE": str(self.archive),
                 "FIXTURE_STAGE": str(self.stage), "FIXTURE_OFFLINE": str(int(offline))},
        )

    def assert_replies(self, result, *expected):
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines(), list(expected))

    def assert_installed(self):
        for name, content in self.contents.items():
            self.assertEqual((self.root / name).read_bytes(), content)
        self.assertEqual((self.root / "user.txt").read_text(encoding="utf-8"), "personal")

    def test_install_now(self):
        self.assert_replies(self.run_helper("NOW"), "READY", "CLOSE", "RELAUNCH")
        self.assert_installed()

    def test_defer_then_install_offline(self):
        self.assert_replies(self.run_helper("LATER"), "READY", "DEFERRED")
        self.assertEqual((self.root / "OkColorPicker.exe").read_bytes(), b"old OkColorPicker.exe")
        self.assert_replies(self.run_helper(offline=True), "CLOSE", "RELAUNCH")
        self.assert_installed()

    def test_ignore_current_or_prerelease_and_reject_corruption(self):
        self.assert_replies(self.run_helper(installed="0.2.0"), "CURRENT")
        release = json.loads(self.feed.read_text(encoding="utf-8"))
        release["prerelease"] = True
        self.feed.write_text(json.dumps(release), encoding="utf-8")
        self.assert_replies(self.run_helper(), "CURRENT")
        self.publish()
        self.archive.write_bytes(b"broken")
        result = self.run_helper("NOW")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("CLOSE", result.stdout)
        self.assertEqual((self.root / "OkColorPicker.exe").read_bytes(), b"old OkColorPicker.exe")

    def test_reject_extra_member_and_rollback_locked_target(self):
        self.contents["../outside.txt"] = b"unexpected"
        self.publish()
        self.assertNotEqual(self.run_helper("NOW").returncode, 0)
        self.assertFalse((self.root / "outside.txt").exists())
        del self.contents["../outside.txt"]
        for name, data in self.contents.items():
            (self.stage / name).write_bytes(data)
        original = {p.name: p.read_bytes() for p in self.root.iterdir() if p.is_file()}
        result = self.run_helper(command=r"""
$busy = [IO.File]::Open((Join-Path $env:FIXTURE_ROOT OkColorPicker.exe),'Open','Read','None')
try { Install-Package $env:FIXTURE_ROOT $env:FIXTURE_STAGE; throw 'Expected sharing violation' }
catch [System.IO.IOException] { 'ROLLED BACK' }
finally { $busy.Dispose() }
""")
        self.assert_replies(result, "ROLLED BACK")
        self.assertEqual({p.name: p.read_bytes() for p in self.root.iterdir() if p.is_file()}, original)
