"""Keep follow-up owners out of existing per-slice coverage denominators."""
import re
import unittest
from pathlib import Path

CI = Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml"
OWNERS = ("reader_policy", "transfer_speed", "recovery_search", "spider_info", "local_pages", "cache_keys", "file_rules", "text_spans", "bbcode", "comic_info_policy", "recovery_scan", "storage_rules", "production_zip", "production_tar", "archive_filters", "webp", "recovery_batch", "recovery_directory", "backup_import")


class CoverageMaskTest(unittest.TestCase):
    def test_followup_masks_cover_both_owner_files_and_test_subdirectories(self):
        masks = re.findall(r"--ignore-filename-regex '([^']*)'", CI.read_text())
        followup = [mask for mask in masks if "/reader_policy(" in mask]
        self.assertGreater(len(followup), 30)
        for mask in followup:
            # LLVM uses POSIX ERE: noncapturing groups invalidate the whole filter.
            self.assertNotIn("(?:", mask)
            self.assertIsNotNone(re.search(mask, "/repo/vendor/zip/src/read.rs"))
            self.assertIsNotNone(re.search(mask, "/repo/crates/codec_limits/src/lib.rs"))
            self.assertIsNotNone(re.search(mask, "/repo/crates/codec_limits/src/tests.rs"))
            for owner in OWNERS:
                with self.subTest(owner=owner,mask=mask[:80]):
                    self.assertIsNotNone(re.search(mask, f"/repo/crates/net/src/{owner}.rs"))
                    self.assertIsNotNone(re.search(mask, f"/repo/crates/net/src/{owner}/tests.rs"))


if __name__ == "__main__":
    unittest.main()
