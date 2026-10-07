"""Original logo source/code-generation regression, not a pixel-equality claim."""

import importlib
import subprocess
import tempfile
import unittest
from pathlib import Path

artwork = importlib.import_module("scripts.export_signin_artwork")
SCRIPT = Path(__file__).resolve().parents[1] / "export_signin_artwork.py"


class OriginalArtworkTest(unittest.TestCase):
    def test_original_source_reproduces_committed_rust_geometry(self):
        self.assertEqual(
            artwork.generate(artwork.SOURCE.read_bytes()).encode("utf-8"),
            artwork.OUTPUT.read_bytes(),
        )
        self.assertIn(artwork.EXPECTED_SHA, artwork.OUTPUT.read_text())

    def test_no_guess_on_changed_original_asset_or_rust_data(self):
        with self.assertRaisesRegex(ValueError, "Canonical logo source drift"):
            artwork.generate(artwork.SOURCE.read_bytes() + b"\n")
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "output.rs"
            output.write_text("stale source")
            process = subprocess.run(
                ["python3", "-B", str(SCRIPT), "--output", str(output), "--check"],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(process.returncode, 0)
            self.assertIn("Generated artwork geometry drift", process.stderr)
            self.assertEqual(output.read_text(), "stale source")

    def test_check_rejects_crlf_byte_drift_without_rewriting(self):
        generated = artwork.generate(artwork.SOURCE.read_bytes()).encode("utf-8")
        crlf = generated.replace(b"\n", b"\r\n")
        self.assertNotEqual(generated, crlf)
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "output.rs"
            output.write_bytes(crlf)
            process = subprocess.run(
                ["python3", "-B", str(SCRIPT), "--output", str(output), "--check"],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(process.returncode, 0)
            self.assertIn("Generated artwork geometry drift", process.stderr)
            self.assertEqual(output.read_bytes(), crlf)

    def test_generation_writes_exact_utf8_bytes_then_checks_unchanged(self):
        generated = artwork.generate(artwork.SOURCE.read_bytes()).encode("utf-8")
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "output.rs"
            for flags in ([], ["--check"]):
                process = subprocess.run(
                    ["python3", "-B", str(SCRIPT), "--output", str(output), *flags],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(process.returncode, 0, process.stderr)
                self.assertEqual(output.read_bytes(), generated)

    def test_absolute_path_subset_preserves_cubic_controls_and_closed_order(self):
        commands = artwork.path_commands(
            "M15,35H11C8.791,35 7,33.209 7,31C7,28.791 8.791,27 11,27H15V35Z"
        )
        self.assertEqual(
            [kind for kind, _ in commands], ["M", "L", "C", "C", "L", "L", "Z"]
        )
        self.assertEqual(commands[2], ("C", [8.791, 35, 7, 33.209, 7, 31]))
        self.assertEqual(commands[-2], ("L", [15, 35]))
        self.assertEqual(
            artwork.path_commands("M11,9L11,54"), [("M", [11, 9]), ("L", [11, 54])]
        )

    def test_unsupported_implicit_relative_and_incomplete_paths_fail_closed(self):
        for value in (
            "m0,0l1,2",
            "M0,0Q1,1 2,2",
            "M1",
            "M0,0 1,1",
            "L0,0",
            "M0,0C1,2 3,4",
            "",
        ):
            with self.subTest(value=value), self.assertRaises(ValueError):
                artwork.path_commands(value)

    def test_numbers_and_colors_never_accept_nonfinite_or_theme_guesses(self):
        for value in ("NaN", "inf", "-inf", "4097", "text"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                artwork.number(value)
        self.assertEqual(artwork.number("-0.5"), -0.5)
        self.assertEqual(artwork.literal(0), "0.0")
        self.assertEqual(artwork.color("#FBBD06"), "[251, 189, 6]")
        for value in ("@color/theme", "#80FF0000", "#FFF", "#GG0000"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                artwork.color(value)

    def test_check_mode_validates_without_writing(self):
        process = subprocess.run(
            ["python3", "-B", str(SCRIPT), "--check"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertIn("Verified original artwork geometry", process.stdout)


if __name__ == "__main__":
    unittest.main()
