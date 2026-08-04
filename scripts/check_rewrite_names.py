#!/usr/bin/env python3
"""Validate names and module paths owned by the Rust-first rewrite.

Rustc and Clippy enforce identifier casing inside Rust code. This gate adds the
repository rules they cannot see: crate/package alignment, source-path naming,
forbidden catch-all modules, and Google-style names in the Kotlin bridge boundary.
The legacy application tree is intentionally outside this ratcheted scope.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path


FORBIDDEN_MODULES = frozenset({"common", "helpers", "manager", "misc", "utils"})
SNAKE_CASE = re.compile(r"^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$")
LOWER_CAMEL_CASE = re.compile(r"^[a-z][A-Za-z0-9]*$")
UPPER_CAMEL_CASE = re.compile(r"^[A-Z][A-Za-z0-9]*$")
UPPER_SNAKE_CASE = re.compile(r"^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)*$")
RUST_MODULE = re.compile(
    r"^\s*(?:#\[[^\n]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?"
    r"mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*[;{]",
    re.MULTILINE,
)
KOTLIN_TYPE = re.compile(
    r"\b(?:data\s+|sealed\s+|enum\s+|annotation\s+|value\s+)?"
    r"(?:class|interface|object|typealias)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
KOTLIN_FUNCTION = re.compile(
    r"\bfun\s+(?:<[^>]+>\s+)?(?:[A-Za-z_][A-Za-z0-9_<>?,.]*\.)?"
    r"([A-Za-z_][A-Za-z0-9_]*)\s*\("
)
KOTLIN_PROPERTY = re.compile(r"\b(?:val|var)\s+([A-Za-z_][A-Za-z0-9_]*)\b")
KOTLIN_BACKTICK_DECLARATION = re.compile(
    r"\b(?:class|interface|object|typealias|fun|val|var)\s+"
    r"(?:<[^>\n]+>\s+)?(?:[A-Za-z_][A-Za-z0-9_<>?,.]*\.)?"
    r"(`[^`\n]+`)"
)
RUST_RAW_STRING = re.compile(r"(?:br|r)(?P<hashes>#{0,255})\"")


def _relative(path: Path, root: Path) -> str:
    """Return a stable repository-relative path for diagnostics."""

    return path.relative_to(root).as_posix()


def _check_domain_name(name: str, label: str, errors: list[str]) -> None:
    if not SNAKE_CASE.fullmatch(name):
        errors.append(f"{label}: expected snake_case domain name, found {name!r}")
    if name in FORBIDDEN_MODULES:
        errors.append(f"{label}: forbidden catch-all module name {name!r}")


def _mask_non_code(text: str, *, kotlin: bool) -> str:
    """Replace comments and string contents with spaces while preserving lines."""

    masked = list(text)
    index = 0
    while index < len(text):
        if text.startswith("//", index):
            end = text.find("\n", index + 2)
            end = len(text) if end < 0 else end
            for position in range(index, end):
                masked[position] = " "
            index = end
            continue

        if text.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < len(text) and depth:
                if text.startswith("/*", end):
                    depth += 1
                    end += 2
                elif text.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            for position in range(index, end):
                if masked[position] != "\n":
                    masked[position] = " "
            index = end
            continue

        if kotlin and text.startswith('"""', index):
            end = text.find('"""', index + 3)
            end = len(text) if end < 0 else end + 3
            for position in range(index, end):
                if masked[position] != "\n":
                    masked[position] = " "
            index = end
            continue

        if not kotlin:
            raw = RUST_RAW_STRING.match(text, index)
            if raw is not None:
                terminator = '"' + raw.group("hashes")
                end = text.find(terminator, raw.end())
                end = len(text) if end < 0 else end + len(terminator)
                for position in range(index, end):
                    if masked[position] != "\n":
                        masked[position] = " "
                index = end
                continue

        if text[index] == '"':
            end = index + 1
            while end < len(text):
                if text[end] == "\\":
                    end = min(end + 2, len(text))
                elif text[end] == '"':
                    end += 1
                    break
                else:
                    end += 1
            for position in range(index, end):
                if masked[position] != "\n":
                    masked[position] = " "
            index = end
            continue

        index += 1
    return "".join(masked)


def _check_rust(root: Path, errors: list[str]) -> None:
    crates = root / "app/src/main/rust/crates"
    if not crates.is_dir():
        errors.append("app/src/main/rust/crates: rewrite crate directory is missing")
        return

    for crate in sorted(path for path in crates.iterdir() if path.is_dir()):
        _check_domain_name(crate.name, _relative(crate, root), errors)
        manifest = crate / "Cargo.toml"
        if not manifest.is_file():
            errors.append(f"{_relative(crate, root)}: Cargo.toml is missing")
            continue
        try:
            with manifest.open("rb") as stream:
                package = tomllib.load(stream).get("package")
        except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
            errors.append(f"{_relative(manifest, root)}: cannot read package metadata: {error}")
            continue
        if not isinstance(package, dict):
            errors.append(
                f"{_relative(manifest, root)}: package metadata must be a TOML table"
            )
            continue
        expected_package = f"eh_{crate.name}"
        if package.get("name") != expected_package:
            errors.append(
                f"{_relative(manifest, root)}: package name must be "
                f"{expected_package!r}, found {package.get('name')!r}"
            )

        source_root = crate / "src"
        if not source_root.is_dir():
            errors.append(f"{_relative(crate, root)}: src directory is missing")
            continue
        for source in sorted(source_root.rglob("*.rs")):
            relative_source = source.relative_to(source_root)
            for directory in relative_source.parts[:-1]:
                _check_domain_name(directory, _relative(source.parent, root), errors)
            if source.stem not in {"lib", "mod", "main"}:
                _check_domain_name(source.stem, _relative(source, root), errors)

            text = _mask_non_code(source.read_text(encoding="utf-8"), kotlin=False)
            for module in RUST_MODULE.findall(text):
                _check_domain_name(module, _relative(source, root), errors)


def _check_kotlin(root: Path, errors: list[str]) -> None:
    bridge = root / "app/src/main/kotlin/com/hippo/ehviewer/bridge"
    if not bridge.is_dir():
        errors.append(f"{_relative(bridge, root)}: Kotlin bridge directory is missing")
        return

    package_root = "com.hippo.ehviewer.bridge"
    for source in sorted(bridge.rglob("*.kt")):
        relative_source = source.relative_to(bridge)
        package_parts = relative_source.parts[:-1]
        for package_part in package_parts:
            if not re.fullmatch(r"[a-z][a-z0-9]*", package_part):
                errors.append(
                    f"{_relative(source, root)}: bridge package component "
                    f"{package_part!r} must be lowercase"
                )
            if package_part in FORBIDDEN_MODULES:
                errors.append(
                    f"{_relative(source, root)}: forbidden bridge package "
                    f"{package_part!r}"
                )

        if not UPPER_CAMEL_CASE.fullmatch(source.stem):
            errors.append(
                f"{_relative(source, root)}: Kotlin bridge filename must be UpperCamelCase"
            )

        text = source.read_text(encoding="utf-8")
        declarations = _mask_non_code(text, kotlin=True)
        package_match = re.search(
            r"^package\s+([A-Za-z0-9_.]+)\s*$", declarations, re.MULTILINE
        )
        expected_package = ".".join((package_root, *package_parts))
        actual_package = package_match.group(1) if package_match else None
        if actual_package != expected_package:
            errors.append(
                f"{_relative(source, root)}: package must be {expected_package!r}, "
                f"found {actual_package!r}"
            )

        for declaration_name in KOTLIN_BACKTICK_DECLARATION.findall(declarations):
            errors.append(
                f"{_relative(source, root)}: backticked declaration "
                f"{declaration_name!r} is forbidden in the Kotlin bridge"
            )
        for type_name in KOTLIN_TYPE.findall(declarations):
            if not UPPER_CAMEL_CASE.fullmatch(type_name):
                errors.append(
                    f"{_relative(source, root)}: Kotlin type {type_name!r} "
                    "must be UpperCamelCase"
                )
        for function_name in KOTLIN_FUNCTION.findall(declarations):
            if not LOWER_CAMEL_CASE.fullmatch(function_name):
                errors.append(
                    f"{_relative(source, root)}: Kotlin function {function_name!r} "
                    "must be lowerCamelCase"
                )
        for property_name in KOTLIN_PROPERTY.findall(declarations):
            if not (
                LOWER_CAMEL_CASE.fullmatch(property_name)
                or UPPER_SNAKE_CASE.fullmatch(property_name)
            ):
                errors.append(
                    f"{_relative(source, root)}: Kotlin property {property_name!r} "
                    "must be lowerCamelCase or UPPER_SNAKE_CASE"
                )


def check_repository(root: Path) -> list[str]:
    """Return every rewrite naming violation below ``root``."""

    errors: list[str] = []
    _check_rust(root, errors)
    _check_kotlin(root, errors)
    return errors


def main() -> int:
    """Run the naming gate against the repository containing this script."""

    root = Path(__file__).resolve().parents[1]
    errors = check_repository(root)
    if errors:
        print("Rust rewrite naming check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Rust rewrite naming check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
