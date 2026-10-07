#!/usr/bin/env python3
"""Negative drift-gate regressions. Never modify cached or original inputs."""
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from check_source import DriftError, select_disassembly, verify

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "native/parity/secure-text"
CACHE = Path.home() / ".gradle/caches/modules-2/files-2.1"


class SourceBindingTests(unittest.TestCase):
    """Exercise fail-closed fixtures and binary/source hash bindings."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=ROOT / "native/build/secure-text")
        self.addCleanup(self.temporary.cleanup)
        self.fixture = Path(self.temporary.name) / "fixture"
        shutil.copytree(FIXTURE, self.fixture)
        self.binding_path = self.fixture / "source-binding.json"
        self.binding = json.loads(self.binding_path.read_text())

    def check_rejected(self):
        """Any changed evidence must reject without exposing input data."""
        self.binding_path.write_text(json.dumps(self.binding))
        with self.assertRaisesRegex(DriftError, "secure-text"):
            verify(ROOT, CACHE, self.fixture)

    def test_source_hash_drift(self):
        self.binding["inputs"][0]["sha256"] = "0" * 64
        self.check_rejected()

    def test_case_fixture_drift(self):
        with (self.fixture / "cases.tsv").open("a") as output:
            output.write("# changed\n")
        self.check_rejected()

    def test_cached_aar_hash_drift(self):
        self.binding["artifacts"][0]["aar_sha256"] = "0" * 64
        self.check_rejected()

    def test_classes_jar_hash_drift(self):
        self.binding["artifacts"][0]["classes_jar_sha256"] = "0" * 64
        self.check_rejected()

    def test_individual_class_hash_drift(self):
        self.binding["artifacts"][0]["classes"][0]["class_sha256"] = "0" * 64
        self.check_rejected()

    def test_bytecode_fixture_drift(self):
        entry = self.binding["artifacts"][0]["classes"][0]
        (self.fixture / entry["fixture"]).write_text("changed\n")
        self.check_rejected()

    def test_regenerated_javap_hash_drift(self):
        self.binding["artifacts"][0]["classes"][0]["fixture_sha256"] = "0" * 64
        self.check_rejected()

    def test_unknown_schema_rejected(self):
        self.binding["schema_version"] = 2
        self.check_rejected()

    def test_malformed_json_redacted(self):
        sentinel = "private malformed evidence"
        self.binding_path.write_text('{"' + sentinel)
        with self.assertRaisesRegex(DriftError, "evidence unavailable or malformed") as error:
            verify(ROOT, CACHE, self.fixture)
        self.assertNotIn(sentinel, str(error.exception))
        self.assertIsNone(error.exception.__cause__)
        self.assertTrue(error.exception.__suppress_context__)

    def test_missing_json_redacted(self):
        self.binding_path.unlink()
        with self.assertRaisesRegex(DriftError, "evidence unavailable or malformed") as error:
            verify(ROOT, CACHE, self.fixture)
        self.assertIsNone(error.exception.__cause__)
        self.assertTrue(error.exception.__suppress_context__)

    def test_nonobject_json_rejected(self):
        for value in [None, [], 1, True, "private evidence"]:
            with self.subTest(kind=type(value).__name__):
                self.binding_path.write_text(json.dumps(value))
                with self.assertRaisesRegex(DriftError, "unsupported secure-text"):
                    verify(ROOT, CACHE, self.fixture)

    def test_invalid_section_rejected_before_hash_comparison(self):
        self.binding["artifacts"][0]["classes"][0]["sections"] = []
        self.check_rejected()


class SectionSelectionTests(unittest.TestCase):
    """Selected evidence must have valid bounds and actual bytecode instructions."""

    disassembly = (
        'Compiled from "Proof.kt"\n'
        "public final class Proof {\n"
        "  public static void check();\n"
        "    Code:\n"
        "         0: return\n"
        "}\n"
    )

    def assert_rejected(self, sections):
        """Reject range before hashing any extracted evidence."""
        with self.assertRaisesRegex(DriftError, "secure-text"):
            select_disassembly(self.disassembly, sections)

    def test_noninteger_bounds_rejected(self):
        for value in ["5", 5.0, None, True, False]:
            for section in [[value, 5], [5, value]]:
                with self.subTest(section=section):
                    self.assert_rejected([section])

    def test_reversed_bounds_rejected(self):
        self.assert_rejected([[5, 4]])

    def test_zero_bounds_rejected(self):
        self.assert_rejected([[0, 5]])
        self.assert_rejected([[5, 0]])

    def test_negative_bounds_rejected(self):
        self.assert_rejected([[-1, 5]])
        self.assert_rejected([[5, -1]])

    def test_out_of_range_bounds_rejected(self):
        self.assert_rejected([[5, 7]])
        self.assert_rejected([[7, 7]])
        android = (FIXTURE / "oracle/BasicSecureTextField_androidKt.javap").read_text()
        with self.assertRaisesRegex(DriftError, "disassembly bounds"):
            select_disassembly(android, [[506, 535]])

    def test_empty_sections_rejected(self):
        self.assert_rejected([])
        self.assert_rejected([[]])

    def test_malformed_sections_rejected(self):
        for sections in ["3:5", {}, [[3]], [[3, 5, 5]], [None], [{"start": 3}]]:
            with self.subTest(sections=sections):
                self.assert_rejected(sections)

    def test_disassembly_free_section_rejected(self):
        self.assert_rejected([[1, 4]])
        self.assert_rejected([[6, 6]])

    def test_each_section_requires_disassembly(self):
        self.assert_rejected([[3, 5], [1, 2]])

    def test_disassembly_free_whole_output_rejected(self):
        for output in ["", "\n", "Compiled from Proof.kt\npublic class Proof {}\n"]:
            with (
                self.subTest(output=output),
                self.assertRaisesRegex(DriftError, "disassembly unavailable"),
            ):
                select_disassembly(output, None)

    def test_valid_section_preserved(self):
        self.assertEqual(
            select_disassembly(self.disassembly, [[3, 5]]),
            "# javap lines 3-5\n"
            "  public static void check();\n"
            "    Code:\n"
            "         0: return\n",
        )

    def test_whole_class_preserved(self):
        self.assertEqual(select_disassembly(self.disassembly, None), self.disassembly)


if __name__ == "__main__":
    unittest.main()
