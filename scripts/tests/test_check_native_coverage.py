"""Regression tests for exact native-owner independent coverage gates."""

import copy
import importlib
import unittest

# Exercise the package entry point, matching test_check_p4_coverage.py.
coverage = importlib.import_module("scripts.check_native_coverage")


def report(count=100, covered=95):
    return {
        "data": [
            {
                "files": [
                    {
                        "filename": "/project/" + coverage.OWNER,
                        "summary": {
                            name: {"count": count, "covered": covered, "percent": 100}
                            for name in ("lines", "functions")
                        },
                    }
                ]
            }
        ]
    }


class NativeCoverageTest(unittest.TestCase):
    def test_exact_threshold_and_zero_misses_pass(self):
        self.assertEqual(coverage.check(report()), {"lines": 95, "functions": 95})
        self.assertEqual(coverage.check(report(7, 7)), {"lines": 100, "functions": 100})

    def test_each_metric_independent_and_reported_percent_not_trusted(self):
        for name in ("lines", "functions"):
            data = report()
            data["data"][0]["files"][0]["summary"][name]["covered"] = 94
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, name):
                coverage.check(data)

    def test_missing_duplicate_and_similar_paths_do_not_pass(self):
        data = report()
        duplicate = copy.deepcopy(data)
        duplicate["data"][0]["files"].append(copy.deepcopy(data["data"][0]["files"][0]))
        similar = copy.deepcopy(data)
        similar["data"][0]["files"][0]["filename"] = (
            "/project/notnative/src/login_state.rs"
        )
        for value in ({"data": []}, duplicate, similar):
            with (
                self.subTest(data=value),
                self.assertRaisesRegex(ValueError, "exactly one"),
            ):
                coverage.check(value)

    def test_relative_and_windows_owner_paths_supported(self):
        for name in (
            coverage.OWNER,
            "./" + coverage.OWNER,
            "C:\\project\\native\\src\\login_state.rs",
        ):
            data = report()
            data["data"][0]["files"][0]["filename"] = name
            self.assertEqual(coverage.check(data)["lines"], 95)

    def test_presentation_owners_independently_require_exact_nonempty_records(self):
        for owner in coverage.OWNERS:
            data = report()
            data["data"][0]["files"][0]["filename"] = "/project/" + owner
            self.assertEqual(coverage.check(data, owner)["lines"], 95)
            data["data"][0]["files"][0]["summary"]["functions"]["covered"] = 94
            with (
                self.subTest(owner=owner),
                self.assertRaisesRegex(ValueError, "functions"),
            ):
                coverage.check(data, owner)
            with self.assertRaisesRegex(ValueError, "exactly one"):
                coverage.check({"data": []}, owner)
        with self.assertRaisesRegex(ValueError, "Unsupported"):
            coverage.check(report(), "native/src/not_an_owner.rs")

    def test_empty_impossible_and_bool_counts_rejected(self):
        for count, covered in ((0, 0), (3, 4), (4, -1), (True, 1), (4, False)):
            with (
                self.subTest(count=count, covered=covered),
                self.assertRaisesRegex(ValueError, "invalid or empty"),
            ):
                coverage.check(report(count, covered))


if __name__ == "__main__":
    unittest.main()
