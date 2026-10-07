#!/usr/bin/env python3
"""Synthetic checker tests, not coverage evidence for Rust or Android behavior."""
import contextlib
import copy
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "preferences_coverage_summary", Path(__file__).with_name("coverage-summary.py")
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("coverage checker module unavailable")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
SENTINEL = "synthetic-private-payload-never-emit"
FUNCTION = {
    "filenames": [str(ROOT / "native/src/preferences.rs")],
    "regions": [[1, 1, 2, 1]],
    "count": 1,
}


class CoverageSummaryTests(unittest.TestCase):
    """Malformed evidence fails closed; no input or path appears in failures."""

    def setUp(self):
        """Create fixture paths under ignored workspace build directory only."""
        build = ROOT / "native/build/preferences-checker"
        build.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=build)
        self.addCleanup(self.temporary.cleanup)
        self.export = Path(self.temporary.name) / f"{SENTINEL}.json"
        self.lines = Path(self.temporary.name) / f"{SENTINEL}.txt"
        self.export.write_text(json.dumps({"data": [{"functions": [FUNCTION]}]}))
        self.lines.write_text("1| 1| synthetic source\n")

    def run_checker(self, arguments=None):
        """Capture result without claiming synthetic percentages are real coverage."""
        output = io.StringIO()
        errors = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
            status = CHECKER.main(
                [str(self.export), str(self.lines)] if arguments is None else arguments
            )
        text = output.getvalue() + errors.getvalue()
        self.assertNotIn(SENTINEL, text)
        self.assertNotIn(str(self.temporary.name), text)
        self.assertNotIn("Traceback", text)
        return status, text

    def test_valid_output_and_duplicate_function_reduction(self):
        """Same source region is counted once across generic instantiations."""
        zero = copy.deepcopy(FUNCTION)
        zero["count"] = 0
        self.export.write_text(json.dumps({"data": [{"functions": [zero, FUNCTION]}]}))
        status, text = self.run_checker()
        self.assertEqual(status, 0)
        self.assertIn("1/1 = 100.00%", text)

    def test_malformed_json_is_redacted(self):
        """JSON parser details never echo payload or paths."""
        self.export.write_text(f'{{"{SENTINEL}":')
        self.assertEqual(self.run_checker()[0], 1)

    def test_missing_and_invalid_unicode_inputs_are_redacted(self):
        """Read failures from either evidence file are caught."""
        for path in (self.export, self.lines):
            original = path.read_bytes()
            with self.subTest(kind="missing", path=path.name):
                path.unlink()
                self.assertEqual(self.run_checker()[0], 1)
            with self.subTest(kind="encoding", path=path.name):
                path.write_bytes(b"\xff")
                self.assertEqual(self.run_checker()[0], 1)
            path.write_bytes(original)

    def test_bad_root_and_empty_evidence_are_rejected(self):
        """Malformed shapes and empty owner functions cannot pass a gate."""
        for export in (None, [], {}, {"data": []}, {"data": [{}]},
                       {"data": [{"functions": []}]}):
            with self.subTest(export=export):
                self.export.write_text(json.dumps(export))
                self.assertEqual(self.run_checker()[0], 1)

    def test_invalid_function_counts_and_regions_are_rejected(self):
        """No boolean, negative, fractional or malformed owner count accepted."""
        for count in (True, -1, 0.5, SENTINEL):
            function = copy.deepcopy(FUNCTION)
            function["count"] = count
            self.export.write_text(json.dumps({"data": [{"functions": [function]}]}))
            self.assertEqual(self.run_checker()[0], 1)
        for region in ([], [1], [True, 1, 2, 1], [-1, 1, 2, 1]):
            function = copy.deepcopy(FUNCTION)
            function["regions"] = [region]
            self.export.write_text(json.dumps({"data": [{"functions": [function]}]}))
            self.assertEqual(self.run_checker()[0], 1)

    def test_empty_and_oversized_numeric_line_evidence_are_rejected(self):
        """Empty or unparseable line counts cannot become coverage success."""
        for text in ("", f"{SENTINEL}\n", "9" * 5000 + "| 1| source\n"):
            self.lines.write_text(text)
            self.assertEqual(self.run_checker()[0], 1)

    def test_below_threshold_is_failure(self):
        """Well-formed zero coverage still fails 95-percent gate."""
        self.lines.write_text("1| 0| source\n")
        status, text = self.run_checker()
        self.assertEqual(status, 1)
        self.assertIn("owner-specific 95% gate failed", text)

    def test_formatted_zero_counts_remain_uncovered(self):
        """Decimal/scaled zero is never treated as an executed line."""
        for count in ("0.0", "0.00k", "00.0M", "0G"):
            self.lines.write_text(f"1| {count}| source\n")
            self.assertEqual(self.run_checker()[0], 1)

    def test_argument_count_is_redacted_failure(self):
        """Missing or extra arguments never produce uncaught exceptions."""
        for arguments in ([], [SENTINEL], [SENTINEL] * 3):
            self.assertEqual(self.run_checker(arguments)[0], 1)


if __name__ == "__main__":
    unittest.main()
