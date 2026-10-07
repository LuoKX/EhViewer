#!/usr/bin/env python3
"""Negative evidence/read/hash/JSON/size/shape failures stay redacted."""
import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
import pathlib
import tempfile
import unittest
from unittest import mock

import evidence


def load_verifier():
    """Load owned sibling explicitly; never resolve an unrelated PATH/package module."""
    try:
        spec = importlib.util.spec_from_file_location("jvm_utf8_verifier", pathlib.Path(__file__).with_name("verify.py"))
        if spec is None or spec.loader is None:
            raise ValueError
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    except (OSError, ImportError, SyntaxError, ValueError):
        raise RuntimeError("synthetic verifier unavailable") from None


verify = load_verifier()


class EvidenceNegatives(unittest.TestCase):
    """Synthetic corruptions only; never alter source, cache, or old goldens."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"])
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.source = self.root / "owner.rs"
        self.source.write_text("fn owner() {}\n")
        self.file = self.root / "synthetic-private-marker"

    def failure(self, arguments):
        output = io.StringIO()
        with contextlib.redirect_stderr(output), contextlib.redirect_stdout(output):
            self.assertEqual(evidence.main(arguments), 1)
        self.assertEqual(output.getvalue(), evidence.FAILURE + "\n")

    def test_arguments_read_failures_and_directory(self):
        for arguments in [[], ["bad"], ["bind"], ["bind", str(self.file)], ["coverage", str(self.root)], ["replay", str(self.file)]]:
            self.failure(arguments)

    def test_malformed_duplicate_nonfinite_nested_and_invalid_utf8_json(self):
        for data in [b"", b"{", b"null", b"[]", b"{\"files\":{},\"files\":{}}", b"NaN", b"Infinity", b"\xff", b"[" * 2000]:
            self.file.write_bytes(data)
            self.failure(["bind", str(self.file)])
            self.failure(["coverage", str(self.file)])

    def test_bounded_reads_reject_size(self):
        self.file.write_bytes(b"x" * (2097152 + 1))
        self.failure(["bind", str(self.file)])
        with self.assertRaises(ValueError):
            evidence.bounded(self.file, 10)
        self.file.write_bytes(b"x" * (evidence.GOLDEN_SIZE + 1))
        self.failure(["replay", str(self.file)])

    def test_hash_source_drift_and_manifest_shape(self):
        actual = hashlib.sha256(self.source.read_bytes()).hexdigest()
        variants = [
            {"files": {}},
            {"files": {}, "snapshot_sha256": "0" * 64},
            {"files": [], "snapshot_sha256": "0" * 64},
            {"files": {str(self.source): "0" * 64}, "snapshot_sha256": "0" * 64},
            {"files": {str(self.source): "bad"}, "snapshot_sha256": "0" * 64},
            {"files": {"../synthetic-private-marker": actual}, "snapshot_sha256": "0" * 64},
            {"files": {str(self.source): actual}, "snapshot_sha256": "0" * 64},
        ]
        for document in variants:
            self.file.write_text(json.dumps(document))
            self.failure(["bind", str(self.file)])

    def test_snapshot_shape_and_identity_drift(self):
        manifest = self.root / "manifest.json"
        for snapshot in [{}, {"tracked_base": "bad", "sha256": {}}, {"tracked_base": "af7c40ea9", "sha256": []}, {"tracked_base": "af7c40ea9", "sha256": {str(i): "0" * 64 for i in range(92)}}]:
            self.file.write_text(json.dumps(snapshot))
            manifest.write_text(json.dumps({"files": {str(self.source): evidence.digest(self.source)}, "snapshot_sha256": evidence.digest(self.file)}))
            with mock.patch.object(evidence, "SNAPSHOT", self.file):
                self.failure(["bind", str(manifest)])

    def test_original_inputs_scope_preserves_history_not_mutable_parent(self):
        original = evidence.document(evidence.SNAPSHOT)
        paths = []

        def original_digest(path):
            name = str(path.relative_to(self.root))
            paths.append(name)
            return original["sha256"][name]

        with mock.patch.object(evidence, "digest", side_effect=original_digest):
            self.assertEqual(evidence.original_inputs(self.root, original), 77)
        self.assertEqual(len(paths), 77)
        self.assertNotIn("native/src/lib.rs", paths)
        self.assertNotIn("native/parity/preferences/coverage-summary.py", paths)
        self.assertNotIn("native/parity/preferences/README.md", paths)
        self.assertIn("native/parity/preferences/goldens/invalid-key.input.bin", paths)
        self.assertIn("native/parity/preferences/javap.txt", paths)
        with mock.patch.object(evidence, "digest", return_value="0" * 64), self.assertRaises(ValueError):
            evidence.original_inputs(self.root, original)
        malformed = copy.deepcopy(original)
        malformed["sha256"].pop("native/parity/preferences/goldens/invalid-key.input.bin")
        malformed["sha256"]["native/private-extra"] = "0" * 64
        with mock.patch.object(evidence, "digest", side_effect=original_digest), self.assertRaises(ValueError):
            evidence.original_inputs(self.root, malformed)
        for snapshot in [None, [], {}, {"tracked_base": "bad"}, {"tracked_base": "af7c40ea9", "sha256": []}]:
            with self.assertRaises(ValueError):
                evidence.original_inputs(self.root, snapshot)
        for key, value in [("../private-marker", "0" * 64), ("/private-marker", "0" * 64), ("invalid", None), ("invalid", "bad")]:
            malformed = copy.deepcopy(original)
            malformed["sha256"].pop("native/src/lib.rs")
            malformed["sha256"][key] = value
            with mock.patch.object(evidence, "digest", side_effect=original_digest), self.assertRaises(ValueError):
                evidence.original_inputs(self.root, malformed)

    def test_document_normalizes_parser_and_read_failures(self):
        for payload in [b"{", b"\xff", b"[" * 2000]:
            self.file.write_bytes(payload)
            with self.assertRaises(ValueError) as caught:
                evidence.document(self.file)
            self.assertEqual(str(caught.exception), "")
            self.assertTrue(caught.exception.__suppress_context__)
        with self.assertRaises(ValueError) as caught:
            evidence.document(self.root / "missing-private-marker")
        self.assertEqual(str(caught.exception), "")

    def test_golden_shape_truncation_count_size_utf16_and_canonical(self):
        empty = b"JU01\xff\0\0\0\0"
        record = b"JU01\x01x\x01\0x\0\x01x\xff\0\0\0\x01"
        self.assertEqual(evidence.parse_golden(empty, 0), 0)
        self.assertEqual(evidence.parse_golden(record, 1), 1)
        for data in [record[:i] for i in range(len(record))] + [
            empty + b"x", b"BAD!" + empty[4:], b"JU01\x41", b"JU01\0\x41",
            b"JU01\0\0\0\xc1", b"JU01\xff\0\0\0\x01",
            record.replace(b"\0x", b"\xd8\0"), record.replace(b"\x01x\xff", b"\x01y\xff"),
            b"x" * (evidence.GOLDEN_SIZE + 1),
        ]:
            with self.assertRaises((ValueError, UnicodeError)):
                evidence.parse_golden(data, 1)
        self.file.write_bytes(record)
        self.failure(["replay", str(self.file)])

    def test_runner_unavailable_and_failed_output_redacted(self):
        records = []
        output = io.StringIO()
        for command in [[str(self.file)], [verify.sys.executable, "-c", "import sys; print('synthetic-private-marker'); sys.exit(1)"]]:
            with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output), self.assertRaises(ValueError):
                verify.run_tool(command, records, dict(os.environ))
        self.assertEqual(output.getvalue(), "")
        self.assertEqual([record["result"] for record in records], ["failed", "failed"])

    def test_runner_unapproved_build_fails_with_generic_diagnostic(self):
        output = io.StringIO()
        with mock.patch.object(verify, "APPROVED_BUILD", self.root / "approved-other"), mock.patch.dict(os.environ, {"JVM_UTF8_BUILD": str(self.file)}), contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            self.assertEqual(verify.main(), 1)
        self.assertEqual(output.getvalue(), verify.FAILURE + "\n")

    def test_runner_jvm_launchers_ignore_path_substitution(self):
        """Intercept selection only; separate poisoned-PATH oracle run proves real JVM."""
        commands = []

        def intercepted(command, _records, _environment, output=None):
            commands.append(command)
            if command == ["rustc", "--print", "sysroot"]:
                return str(self.root).encode()
            return b""

        output = io.StringIO()
        with (
            mock.patch.dict(os.environ, {"JVM_UTF8_BUILD": str(self.root), "PATH": str(self.root)}),
            mock.patch.object(verify, "APPROVED_BUILD", self.root),
            mock.patch.object(evidence, "bind", return_value=19),
            mock.patch.object(evidence, "replay", return_value=evidence.GOLDEN_CASES),
            mock.patch.object(evidence, "coverage", return_value=(61, 62, 6, 6)),
            mock.patch.object(verify, "run_tool", side_effect=intercepted),
            mock.patch.object(verify, "oracle_negative", side_effect=lambda command, *_: commands.append(command)),
            contextlib.redirect_stdout(output),
        ):
            self.assertEqual(verify.main(), 0)
        self.assertEqual(commands[0][0], "/usr/lib/jvm/java-25-openjdk-amd64/bin/javac")
        java_commands = [command for command in commands if "Oracle" in command]
        self.assertEqual(len(java_commands), 4)
        self.assertTrue(all(command[0] == "/usr/lib/jvm/java-25-openjdk-amd64/bin/java" for command in java_commands))
        self.assertFalse(any(command[0] in {"java", "javac"} for command in commands))

    def test_runner_rejected_directory_and_symlink_leave_no_ledger(self):
        allowed = self.root / "allowed"
        outside = self.root / "outside-private"
        allowed.mkdir()
        outside.mkdir()
        escape = allowed / "escape"
        escape.symlink_to(outside, target_is_directory=True)
        for requested in [outside, escape, allowed / ".." / "outside-private"]:
            output = io.StringIO()
            with mock.patch.object(verify, "APPROVED_BUILD", allowed), mock.patch.dict(os.environ, {"JVM_UTF8_BUILD": str(requested)}), contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
                self.assertEqual(verify.main(), 1)
            self.assertEqual(output.getvalue(), verify.FAILURE + "\n")
            self.assertFalse((outside / "commands-results.json").exists())
            self.assertEqual(list(outside.iterdir()), [])

    def test_frozen_golden_json_shape_count_and_hash(self):
        original = json.loads((pathlib.Path(__file__).parent / "frozen-jvm.json").read_text())
        self.file.write_text(json.dumps(original))
        evidence.frozen(self.file)
        for key, value in [("format", "bad"), ("byte_length", 1), ("cases", True), ("sha256", "0" * 64), ("record", []), ("families", []), ("families", {"empty": 1})]:
            document = dict(original, **{key: value})
            self.file.write_text(json.dumps(document))
            with self.assertRaises(ValueError):
                evidence.frozen(self.file)
        self.file.write_text("[]")
        with self.assertRaises(ValueError):
            evidence.frozen(self.file)

    def test_coverage_owner_missing_counts_shapes_below_gate(self):
        owner = {"filename": str(self.source), "summary": {"lines": {"count": 1, "covered": 1}}}
        function = {"filenames": [str(self.source)], "regions": [[1, 1, 1, 13]], "count": 1}
        base = {"type": "llvm.coverage.json.export", "data": [{"files": [owner], "functions": [function]}]}
        variants = [{}, {"type": "bad", "data": []}]
        for field, value in [("count", 0), ("count", True), ("count", "private"), ("covered", 2), ("covered", -1), ("covered", 0)]:
            document = copy.deepcopy(base)
            document["data"][0]["files"][0]["summary"]["lines"][field] = value
            variants.append(document)
        for region in [[], [0, 1, 1, 13], [1, 1, 2, 13], [True, 1, 1, 13], [1, "private", 1, 13]]:
            document = copy.deepcopy(base)
            document["data"][0]["functions"][0]["regions"] = [region]
            variants.append(document)
        for count in [0, -1, True, "private"]:
            document = copy.deepcopy(base)
            document["data"][0]["functions"][0]["count"] = count
            variants.append(document)
        for key, value in [("functions", []), ("files", []), ("files", [owner, owner]), ("functions", [{"filenames": []}])]:
            document = copy.deepcopy(base)
            document["data"][0][key] = value
            variants.append(document)
        # Actual public main infers production owner, so patch only test fixture path.
        for document in variants:
            self.file.write_text(json.dumps(document))
            with self.assertRaises((ValueError, KeyError, IndexError, TypeError)):
                evidence.coverage(self.file, self.source)
            self.failure(["coverage", str(self.file)])


if __name__ == "__main__":
    unittest.main()
