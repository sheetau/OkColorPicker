"""Check the development tools and native linker without creating product code."""
from pathlib import Path
import os
import sqlite3
import subprocess
import sys
import tempfile


def run(*command):
    result = subprocess.run(command, check=True, capture_output=True, text=True, encoding="utf-8")
    print(result.stdout.strip())
    return result.stdout


def main():
    if sys.version_info < (3, 11):
        raise RuntimeError("Python 3.11 or later is required")
    print(f"Python {sys.version.split()[0]}, SQLite {sqlite3.sqlite_version}")
    if not run("rustc", "--version").startswith("rustc 1.98.0 "):
        raise RuntimeError("Rust 1.98.0 is required")
    for command in [("cargo", "--version"), ("cargo", "fmt", "--version"),
                    ("cargo", "clippy", "--version"), ("git", "--version")]:
        run(*command)
    with tempfile.TemporaryDirectory(prefix="okcolorpicker-probe-") as directory:
        root = Path(directory)
        source = root / "probe.rs"
        binary = root / ("probe.exe" if os.name == "nt" else "probe")
        source.write_text('fn main() { println!("Rust native toolchain ready"); }\n', encoding="utf-8")
        run("rustc", "--edition", "2024", str(source), "-o", str(binary))
        run(str(binary))


if __name__ == "__main__":
    main()
