"""Keep native ABI checks on deterministic, current AGP artifacts."""

import re
import unittest
from pathlib import Path


WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml"


class NativeJniCiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        text = WORKFLOW.read_text()
        marker = "      - name: Android JNI Symbol Check\n"
        if text.count(marker) != 1:
            raise AssertionError("Expected one Android native symbol gate")
        cls.step = text.split(marker, 1)[1].split("\n  default:", 1)[0]

    def test_android_symbol_sort_and_comm_use_c_locale(self):
        export = self.step.index("export LC_ALL=C")
        self.assertLess(export, self.step.index("sort -u"))
        self.assertLess(export, self.step.index("comm -12"))

    def test_native_gate_uses_packaged_and_unstripped_merged_libraries(self):
        expected = {
            "native_library": (
                "app/build/intermediates/stripped_native_libs/defaultDebug/"
                "stripDefaultDebugDebugSymbols/out/lib/x86_64/libehviewer.so"
            ),
            "unstripped_native_library": (
                "app/build/intermediates/merged_native_libs/defaultDebug/"
                "mergeDefaultDebugNativeLibs/out/lib/x86_64/libehviewer.so"
            ),
        }
        for variable, path in expected.items():
            with self.subTest(variable=variable):
                match = re.search(
                    rf'\b{variable}="\$\(printf \'%s\' \\\n(.*?)\)"',
                    self.step,
                    re.DOTALL,
                )
                if match is None:
                    raise AssertionError(f"Missing native library assignment: {variable}")
                pieces = re.findall(r"'([^']*)'", match.group(1))
                self.assertEqual(path, "".join(pieces))
        self.assertIn('nm --defined-only "$unstripped_native_library"', self.step)
        self.assertIn("android-retired-native-symbols.txt", self.step)


if __name__ == "__main__":
    unittest.main()
