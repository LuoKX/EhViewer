"""Keep follow-up owners out of existing per-slice coverage denominators."""
import re
import unittest
from pathlib import Path

CI = Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml"
OWNERS = (
    "reader_policy", "transfer_speed", "recovery_search", "spider_info",
    "local_pages", "cache_keys", "file_rules", "text_spans", "bbcode",
    "comic_info_policy", "recovery_scan", "storage_rules", "production_zip",
    "production_tar", "archive_filters", "webp", "recovery_batch",
    "recovery_directory", "backup_import", "download_metadata",
)
MASK_ARGUMENT = re.compile(
    r"""--ignore-filename-regex\s+('[^']*'|"\$\(printf '%s'\s+.*?\)")""",
    re.DOTALL,
)


def coverage_masks(source):
    """Read literal or printf-concatenated masks without running workflow code."""
    masks = []
    for match in MASK_ARGUMENT.finditer(source):
        argument = match.group(1)
        if argument.startswith("'"):
            masks.append(argument[1:-1])
            continue
        body = argument[len('"$(printf \'%s\''): -2]
        fragments = re.findall(r"'([^']*)'", body)
        remainder = re.sub(r"'[^']*'", "", body)
        if not fragments or re.sub(r"[\s\\]", "", remainder):
            raise ValueError("Unsupported coverage mask expression")
        masks.append("".join(fragments))
    return masks


class CoverageMaskTest(unittest.TestCase):
    def test_followup_masks_cover_both_owner_files_and_test_subdirectories(self):
        masks = coverage_masks(CI.read_text())
        self.assertEqual(len(masks), 42)
        followup = [mask for mask in masks if "/reader_policy(" in mask]
        self.assertGreater(len(followup), 30)
        for mask in followup:
            # LLVM uses POSIX ERE: noncapturing groups invalidate the whole filter.
            self.assertNotIn("(?:", mask)
            self.assertIsNotNone(re.search(mask, "/repo/vendor/zip/src/read.rs"))
            self.assertIsNotNone(re.search(mask, "/repo/crates/codec_limits/src/lib.rs"))
            self.assertIsNotNone(re.search(mask, "/repo/crates/codec_limits/src/tests.rs"))
            for owner in OWNERS:
                with self.subTest(owner=owner, mask=mask[:80]):
                    self.assertIsNotNone(re.search(mask, f"/repo/crates/net/src/{owner}.rs"))
                    self.assertIsNotNone(re.search(mask, f"/repo/crates/net/src/{owner}/tests.rs"))

    def test_literal_mask_remains_supported(self):
        self.assertEqual(coverage_masks("--ignore-filename-regex '(/a|/b\\.rs$)'"),
                         [r"(/a|/b\.rs$)"])

    def test_wrapped_mask_keeps_every_byte(self):
        source = "--ignore-filename-regex \"$(printf '%s' \\\n  '(/a|' \\\n  '/b\\.rs$)')\""
        self.assertEqual(coverage_masks(source), [r"(/a|/b\.rs$)"])

    def test_wrapped_mask_rejects_nonliteral_arguments(self):
        source = "--ignore-filename-regex \"$(printf '%s' \\\n  '(/a)' $UNTRUSTED)\""
        with self.assertRaisesRegex(ValueError, "Unsupported coverage mask expression"):
            coverage_masks(source)


if __name__ == "__main__":
    unittest.main()
