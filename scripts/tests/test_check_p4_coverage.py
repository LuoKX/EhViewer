"""Regression tests for package and direct-script coverage gate entry points."""

import importlib
import runpy
import sys
from pathlib import Path
from unittest import TestCase, mock

from scripts import check_p3_coverage

SCRIPT = Path(__file__).resolve().parents[1] / "check_p4_coverage.py"


class P4CoverageEntryPointTest(TestCase):
    def test_can_be_imported_as_package_module(self):
        module = importlib.import_module("scripts.check_p4_coverage")
        self.assertIs(module.main, check_p3_coverage.main)

    def test_module_entry_point_runs_p4_owners(self):
        with (
            mock.patch.object(check_p3_coverage, "main") as gate,
            mock.patch.dict(sys.modules),
        ):
            sys.modules.pop("scripts.check_p4_coverage", None)
            namespace = runpy.run_module("scripts.check_p4_coverage", run_name="__main__")
        gate.assert_called_once_with(namespace["OWNERS"], "p4")

    def test_direct_script_entry_point_runs_same_gate(self):
        with (
            mock.patch.object(check_p3_coverage, "main") as gate,
            mock.patch.dict(sys.modules, {"check_p3_coverage": check_p3_coverage}),
            mock.patch.object(sys, "path", [str(SCRIPT.parent), *sys.path]),
        ):
            namespace = runpy.run_path(str(SCRIPT), run_name="__main__")
            self.assertEqual(sys.path[0], str(SCRIPT.parent.parent))
        gate.assert_called_once_with(namespace["OWNERS"], "p4")
