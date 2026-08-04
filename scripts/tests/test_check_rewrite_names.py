"""Regression tests for the Rust rewrite naming gate."""

from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase

from scripts.check_rewrite_names import check_repository


class RewriteNameCheckTest(TestCase):
    """Exercise accepted and rejected repository shapes."""

    def _write(self, root: Path, relative_path: str, content: str) -> None:
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def _valid_root(self, root: Path) -> None:
        self._write(
            root,
            "app/src/main/rust/crates/domain/Cargo.toml",
            '[package]\nname = "eh_domain"\n',
        )
        self._write(
            root,
            "app/src/main/rust/crates/domain/src/lib.rs",
            "pub mod session;\n",
        )
        self._write(
            root,
            "app/src/main/rust/crates/domain/src/session.rs",
            'const EXAMPLE: &str = r#"\nmod helpers;\n"#;\n'
            "/* mod manager; */\n"
            "pub struct SessionState;\n",
        )
        self._write(
            root,
            "app/src/main/kotlin/com/hippo/ehviewer/bridge/auth/SessionBridge.kt",
            "package com.hippo.ehviewer.bridge.auth\n"
            "internal data class SessionState(val signedIn: Boolean)\n"
            "internal fun readSession(): SessionState = SessionState(false)\n",
        )

    def test_accepts_domain_names_and_google_style_bridge_names(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self._valid_root(root)
            self._write(
                root,
                "app/src/main/kotlin/com/hippo/ehviewer/bridge/auth/CommentedNames.kt",
                "package com.hippo.ehviewer.bridge.auth\n"
                "/** Describes a class named bad_type and fun BadFunction(). */\n"
                'private const val EXAMPLE = "fun BadFunction()"\n'
                'private const val MULTILINE = """\nclass bad_type\n"""\n'
                "internal object CommentedNames\n",
            )
            self.assertEqual(check_repository(root), [])

    def test_rejects_catch_all_rust_modules_and_wrong_crate_name(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self._valid_root(root)
            self._write(
                root,
                "app/src/main/rust/crates/domain/Cargo.toml",
                '[package]\nname = "wrong_name"\n',
            )
            self._write(
                root,
                "app/src/main/rust/crates/domain/src/helpers.rs",
                "pub fn helper() {}\n",
            )
            self._write(
                root,
                "app/src/main/rust/crates/domain/src/lib.rs",
                "#[cfg(test)] pub mod utils;\n",
            )
            self._write(
                root,
                "app/src/main/rust/crates/empty/Cargo.toml",
                '[package]\nname = "eh_empty"\n',
            )
            self._write(
                root,
                "app/src/main/rust/crates/broken/Cargo.toml",
                "not valid toml = [\n",
            )
            self._write(
                root,
                "app/src/main/rust/crates/scalar/Cargo.toml",
                'package = "eh_scalar"\n',
            )
            invalid_encoding = (
                root / "app/src/main/rust/crates/encoded/Cargo.toml"
            )
            invalid_encoding.parent.mkdir(parents=True, exist_ok=True)
            invalid_encoding.write_bytes(b"\xff")
            errors = check_repository(root)
            self.assertTrue(any("eh_domain" in error for error in errors))
            self.assertTrue(any("forbidden catch-all" in error for error in errors))
            self.assertTrue(any("module name 'utils'" in error for error in errors))
            self.assertTrue(any("src directory is missing" in error for error in errors))
            self.assertTrue(any("cannot read package metadata" in error for error in errors))
            self.assertTrue(
                any(
                    "encoded/Cargo.toml: cannot read package metadata" in error
                    for error in errors
                )
            )
            self.assertTrue(any("package metadata must be" in error for error in errors))

    def test_rejects_bad_kotlin_file_package_and_declaration_names(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self._valid_root(root)
            self._write(
                root,
                "app/src/main/kotlin/com/hippo/ehviewer/bridge/auth/bad_name.kt",
                "package com.hippo.ehviewer.bridge.wrong\n"
                "internal class bad_type\n"
                "internal fun BadFunction() = Unit\n"
                "internal fun `Bad function`() = Unit\n",
            )
            errors = check_repository(root)
            self.assertTrue(any("filename" in error for error in errors))
            self.assertTrue(any("package must be" in error for error in errors))
            self.assertTrue(any("Kotlin type" in error for error in errors))
            self.assertTrue(any("Kotlin function" in error for error in errors))
            self.assertTrue(any("backticked declaration" in error for error in errors))
