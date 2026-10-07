#!/usr/bin/env python3
"""Negative tests for redacted filesystem evidence; synthetic percentages only."""
import contextlib
import hashlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

BASE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("filesystem_coverage", BASE / "coverage.py")
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("coverage checker unavailable")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
MODULES = {}
for name in ["prepare", "verify"]:
    spec = importlib.util.spec_from_file_location(name, BASE / f"{name}.py")
    if spec is None or spec.loader is None:
        raise RuntimeError("evidence module unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    MODULES[name] = module
SENTINEL = "synthetic-private-never-echo"


class EvidenceTests(unittest.TestCase):
    """Malformed reads/counts fail without input, exception or path output."""
    def setUp(self):
        (BASE / "build/checker-tests").mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=BASE / "build/checker-tests")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.paths = [root / f"{SENTINEL}-{i}" for i in range(3)]
        self.sources = [(root / "owner1.rs").resolve(), (root / "owner2.rs").resolve()]
        self.functions = [{"filenames": [str(p)], "regions": [[1, 1, 2, 1]], "count": 1} for p in self.sources]
        self.paths[0].write_text(json.dumps({"data": [{"functions": self.functions}]}))
        for p in self.paths[1:]:
            p.write_text("1| 1| synthetic\n")
        self.scope = [("preferences", self.sources[0], 1, 2, []), ("preference_lock", self.sources[1], 1, 2, [])]

    def invoke(self):
        out, err = io.StringIO(), io.StringIO()
        with patch.object(CHECKER, "scopes", return_value=self.scope), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = CHECKER.main([str(p) for p in self.paths])
        text = out.getvalue() + err.getvalue()
        self.assertNotIn(SENTINEL, text)
        self.assertNotIn(self.temp.name, text)
        self.assertNotIn("Traceback", text)
        return status

    def test_baseline_and_command_json_errors_are_payload_free(self):
        parser = MODULES["verify"].load_json
        self.assertEqual(parser(b'{"executable":null}'), {"executable": None})
        for raw in [('{"' + SENTINEL).encode(), b'\xff', b'[' * 4000]:
            with self.assertRaises(ValueError) as caught:
                parser(raw)
            self.assertEqual(str(caught.exception), "invalid evidence")
            self.assertTrue(caught.exception.__suppress_context__)
            self.assertIsNone(caught.exception.__cause__)

    def test_valid_and_duplicate_regions(self):
        duplicate = dict(self.functions[0], count=0)
        self.paths[0].write_text(json.dumps({"data": [{"functions": [duplicate, *self.functions]}]}))
        self.assertEqual(self.invoke(), 0)

    def test_missing_malformed_and_encoding_failure(self):
        for path in self.paths:
            original = path.read_bytes()
            path.unlink()
            self.assertEqual(self.invoke(), 1)
            path.write_bytes(b"\xff")
            self.assertEqual(self.invoke(), 1)
            path.write_bytes(original)
        self.paths[0].write_text(f'{{"{SENTINEL}":')
        self.assertEqual(self.invoke(), 1)

    def test_empty_and_invalid_shapes(self):
        for value in [None, {}, {"data": []}, {"data": [{}]}, {"data": [{"functions": []}]}]:
            self.paths[0].write_text(json.dumps(value))
            self.assertEqual(self.invoke(), 1)

    def test_invalid_counts_and_regions(self):
        for count in [True, -1, 0.5, SENTINEL]:
            bad = dict(self.functions[0], count=count)
            self.paths[0].write_text(json.dumps({"data": [{"functions": [bad, self.functions[1]]}]}))
            self.assertEqual(self.invoke(), 1)
        for region in [[], [1], [True, 1, 2, 1], [-1, 1, 2, 1]]:
            bad = dict(self.functions[0], regions=[region])
            self.paths[0].write_text(json.dumps({"data": [{"functions": [bad, self.functions[1]]}]}))
            self.assertEqual(self.invoke(), 1)

    def test_preparation_missing_malformed_and_encoding_failures_redacted(self):
        root = Path(self.temp.name)
        (root / "native").mkdir()
        lock = root / "native/Cargo.lock"
        for content in [None, b"\xff", f'[\"{SENTINEL}\"'.encode(), b"wrong=1"]:
            if content is None:
                if lock.exists():
                    lock.unlink()
            else:
                lock.write_bytes(content)
            out, err = io.StringIO(), io.StringIO()
            with patch.object(MODULES["prepare"], "ROOT", root), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                status = MODULES["prepare"].main()
            self.assertEqual(status, 1)
            text = out.getvalue() + err.getvalue()
            self.assertNotIn(SENTINEL, text)
            self.assertNotIn(self.temp.name, text)
            self.assertNotIn("Traceback", text)

    def test_runner_read_and_parse_failures_redacted(self):
        for error in [OSError(SENTINEL), ValueError(SENTINEL), UnicodeError(SENTINEL)]:
            out, err = io.StringIO(), io.StringIO()
            with patch.object(MODULES["verify"], "verify", side_effect=error), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                status = MODULES["verify"].main()
            self.assertEqual(status, 1)
            self.assertNotIn(SENTINEL, out.getvalue() + err.getvalue())
            self.assertNotIn("Traceback", out.getvalue() + err.getvalue())

    def source_fixture(self):
        """Copy actual immutable lock source/test into isolated binding fixture."""
        root = Path(self.temp.name) / "source-tree"
        names = ["native/src/preference_lock.rs", "native/tests/preference_lock.rs"]
        rows = []
        for name in names:
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            data = (MODULES["verify"].ROOT / name).read_bytes()
            target.write_bytes(data)
            rows.append(f"{hashlib.sha256(data).hexdigest()}  {name}")
        manifests = ["native/parity/preferences/changed-source-sha256.txt", "native/parity/preferences/source-sha256.txt", "native/parity/preferences/evidence-sha256.txt", "native/parity/preference-lock/evidence-sha256.txt"]
        for name in manifests:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("\n".join(rows) + "\n" if name.endswith("changed-source-sha256.txt") else "")
        return root, names

    def source_status(self, root):
        """Run actual consumed-manifest checker through redacted entry point."""
        out, err = io.StringIO(), io.StringIO()
        def check():
            MODULES["verify"].source_manifests(root)
        with patch.object(MODULES["verify"], "verify", side_effect=check), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = MODULES["verify"].main()
        text = out.getvalue() + err.getvalue()
        self.assertNotIn(SENTINEL, text)
        self.assertNotIn(self.temp.name, text)
        self.assertNotIn("Traceback", text)
        return status

    def test_actual_lock_source_and_test_hash_mismatches_refused(self):
        root, names = self.source_fixture()
        self.assertEqual(self.source_status(root), 0)
        for name in names:
            path = root / name
            original = path.read_bytes()
            path.write_bytes(original + f"\n// {SENTINEL}\n".encode())
            self.assertEqual(self.source_status(root), 1)
            path.write_bytes(original)
        self.assertEqual(self.source_status(root), 0)

    def test_missing_required_source_bindings_refused(self):
        root, names = self.source_fixture()
        manifest = root / "native/parity/preferences/changed-source-sha256.txt"
        rows = manifest.read_text().splitlines()
        for name in names:
            manifest.write_text("\n".join(row for row in rows if not row.endswith(name)) + "\n")
            self.assertEqual(self.source_status(root), 1)
        manifest.write_text("\n".join(rows) + "\n")
        self.assertEqual(self.source_status(root), 0)

    def test_generated_cache_binding_refused(self):
        root, _ = self.source_fixture()
        path = root / "native/parity/preference-lock/__pycache__/private.pyc"
        path.parent.mkdir(parents=True, exist_ok=True)
        data = SENTINEL.encode()
        path.write_bytes(data)
        manifest = root / "native/parity/preference-lock/evidence-sha256.txt"
        manifest.write_text(f"{hashlib.sha256(data).hexdigest()}  {path.relative_to(root)}\n")
        self.assertEqual(self.source_status(root), 1)

    def test_production_scope_excludes_test_only_name_accessor(self):
        scope = next(item for item in CHECKER.scopes() if item[0] == "preference_lock")
        text = scope[1].read_text().splitlines()
        helper = text.index("        pub(crate) fn name(&self) -> &str {") + 1
        end = next(index + 1 for index in range(helper, len(text)) if text[index] == "        }")
        self.assertTrue(any(a <= helper - 1 and end <= b for a, b in scope[4]))
        production = text.index("        pub(crate) fn sync(&self) -> CommitOutcome {") + 1
        self.assertFalse(any(a <= production <= b for a, b in scope[4]))

    def test_formatted_zero_and_empty_lines_fail(self):
        for text in ["", SENTINEL, "1| 0| source", "1| 0.00k| source", "1| 00.0M| source", "9" * 5000 + "| 1| source"]:
            self.paths[1].write_text(text)
            self.assertEqual(self.invoke(), 1)


if __name__ == "__main__":
    unittest.main()
