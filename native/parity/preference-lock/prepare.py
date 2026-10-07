#!/usr/bin/env python3
"""Build-only private cached dependency closure; generic failures, no downloads."""
import hashlib
import json
import sys
import tarfile
import tomllib
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[3]
BUILD = ROOT / "native/parity/preference-lock/build"

def prepare():
    """Copy only already-locked cached crates into authorized private build."""
    lock = tomllib.loads((ROOT / "native/Cargo.lock").read_text())
    packages = lock["package"]
    selected = {}
    def visit(name, version=None):
        matches = [p for p in packages if p["name"] == name and (version is None or p["version"] == version)]
        if len(matches) != 1:
            raise ValueError
        p = matches[0]
        key = (p["name"], p["version"])
        if key in selected:
            return
        selected[key] = p
        for dep in p.get("dependencies", []):
            fields = dep.split()
            visit(fields[0], fields[1] if len(fields) > 1 else None)
    visit("rustix", "1.1.5")
    cache = Path("/home/ngnl/.cargo/registry")
    vendor = BUILD / "vendor"
    vendor.mkdir(parents=True, exist_ok=True)
    inputs = []
    for (name, version), package in sorted(selected.items()):
        filename = f"{name}-{version}"
        archives = list((cache / "cache").glob(f"*/{filename}.crate"))
        if len(archives) != 1:
            raise ValueError
        checksum = hashlib.sha256(archives[0].read_bytes()).hexdigest()
        if checksum != package["checksum"]:
            raise ValueError
        destination = vendor / filename
        destination.mkdir(exist_ok=True)
        file_hashes = {}
        with tarfile.open(archives[0], "r:gz") as archive:
            for member in archive:
                parts = PurePosixPath(member.name).parts
                if not parts or parts[0] != filename or any(part in ("..", "") for part in parts):
                    raise ValueError
                relative = Path(*parts[1:])
                if member.isdir():
                    (destination / relative).mkdir(parents=True, exist_ok=True)
                    continue
                if not member.isfile() or member.size > 134_217_728:
                    raise ValueError
                stream = archive.extractfile(member)
                if stream is None:
                    raise ValueError
                data = stream.read()
                target = destination / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
                file_hashes[relative.as_posix()] = hashlib.sha256(data).hexdigest()
        (destination / ".cargo-checksum.json").write_text(json.dumps({"files": file_hashes, "package": checksum}))
        inputs.append({"name": name, "version": version, "sha256": checksum})
    harness = BUILD / "harness"
    (harness / "src").mkdir(parents=True, exist_ok=True)
    (harness / ".cargo").mkdir(exist_ok=True)
    (BUILD / "cargo-home").mkdir(exist_ok=True)
    (BUILD / "tmp").mkdir(exist_ok=True)
    manifest = '''[package]
name = "preference_filesystem_harness"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[target.'cfg(any(target_os = "linux", target_os = "android"))'.dependencies]
rustix = { version = "=1.1.5", default-features = false, features = ["fs", "std"] }
[lints.rust]
missing_docs = "deny"
unsafe_code = "deny"
rust_2018_idioms = { level = "deny", priority = -1 }
unused_lifetimes = "deny"
[lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "deny", priority = -1 }
'''
    for name in ["preferences", "preference_lock"]:
        manifest += f'\n[[test]]\nname = "{name}"\npath = "{ROOT / "native/tests" / (name + ".rs")}"\n'
    (harness / "Cargo.toml").write_text(manifest)
    source = "//! Isolated safe filesystem/codec test harness; not application runtime.\n"
    for name in ["preference_lock", "preferences"]:
        source += f'#[path = "{ROOT / "native/src" / (name + ".rs")}"]\npub mod {name};\n'
    (harness / "src/lib.rs").write_text(source)
    config = f'''[source.crates-io]
replace-with = "private-cached-closure"
[source.private-cached-closure]
directory = "{vendor.resolve()}"
[build]
jobs = 2
target-dir = "{(BUILD / "target").resolve()}"
'''
    (harness / ".cargo/config.toml").write_text(config)
    (BUILD / "dependency-inputs.json").write_text(json.dumps(inputs, indent=2) + "\n")
    print("Private locked-cache closure prepared; no registry writes or downloads.")

def main():
    """Return redacted preparation status without traceback or parser details."""
    try:
        prepare()
    except (OSError, ValueError, KeyError, IndexError, TypeError, tarfile.TarError, tomllib.TOMLDecodeError):
        print("FAIL: private dependency preparation unavailable or malformed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
