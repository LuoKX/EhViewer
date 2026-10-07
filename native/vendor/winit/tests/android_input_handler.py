"""Source contract checks for Android's single raw-input dispatch seam.

Run with Python's stdlib unittest; no Android runtime or user input needed.
"""

import hashlib
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
PUBLIC = (ROOT / "src/platform/android.rs").read_text()
BACKEND = (ROOT / "src/platform_impl/android/mod.rs").read_text()
DISPATCH = BACKEND.split("    fn handle_input_event<F>(", 1)[1].split("    pub fn run<F>(", 1)[0]


class AndroidInputHandlerContract(unittest.TestCase):
    def test_single_original_iterator_owner(self):
        self.assertEqual(BACKEND.count(".input_events_iter()"), 1)
        self.assertIn(
            "input_iter.next(|event| self.handle_input_event(&android_app, event, callback))",
            BACKEND,
        )
        self.assertNotIn("input_events_iter", PUBLIC)

    def test_handler_once_before_default_dispatch(self):
        self.assertEqual(DISPATCH.count("(handler.0)(event)"), 1)
        hook = DISPATCH.index("if let Some(handler) = &self.android_input_handler")
        chosen_status = DISPATCH.index("return status;")
        default = DISPATCH.index("let mut input_status = InputStatus::Handled;")
        self.assertLess(hook, chosen_status)
        self.assertLess(chosen_status, default)
        self.assertIn("if let Some(status) = (handler.0)(event)", DISPATCH)

    def test_public_bounds_and_android_only_cfg(self):
        bounds = re.compile(
            r"F: Fn\(&activity::input::InputEvent<'_>\) -> Option<activity::InputStatus>"
            r"\s*\+ Send\s*\+ Sync\s*\+ 'static"
        )
        self.assertEqual(len(bounds.findall(PUBLIC)), 2)
        self.assertEqual(
            PUBLIC.count("#[cfg(android_platform)]\n    fn with_android_input_handler<F>"),
            2,
        )
        self.assertIn("Arc::new(handler)", PUBLIC)
        self.assertIn("android_input_handler: attributes.android_input_handler.clone()", BACKEND)
        self.assertIn("android_input_handler: None", BACKEND)

    def test_clone_debug_and_pointer_equality(self):
        self.assertIn("#[derive(Clone)]\npub(crate) struct AndroidInputHandler", BACKEND)
        wrapper = BACKEND.split("type AndroidInputHandlerFn =", 1)[1].split(
            "#[derive(Debug, Clone, PartialEq)]", 1
        )[0]
        self.assertIn("Arc<AndroidInputHandlerFn>", wrapper)
        self.assertIn('f.write_str("AndroidInputHandler")', wrapper)
        self.assertIn("Arc::ptr_eq(&self.0, &other.0)", wrapper)
        self.assertNotIn("write!", wrapper)
        self.assertIn(
            "#[derive(Debug, Clone, PartialEq)]\npub(crate) struct PlatformSpecificEventLoopAttributes",
            BACKEND,
        )

    def test_text_and_unknown_events_explicitly_unhandled_without_logging(self):
        default = DISPATCH.split("// No built-in text/editor-action dispatch.", 1)[1]
        self.assertIn("InputEvent::TextEvent(_) | InputEvent::TextAction(_)", default)
        self.assertEqual(default.count("input_status = InputStatus::Unhandled;"), 2)
        self.assertNotRegex(default, r"(?:trace|debug|info|warn|error)!|\{event[^}]*\}")
        self.assertNotRegex(DISPATCH, r"\{event:?[^}]*\}|event\s*,\s*event\b")

    def test_original_motion_key_volume_dispatch_unchanged(self):
        original_dispatch = DISPATCH.split("        match event {", 1)[1].split(
            "            // No built-in text/editor-action dispatch.", 1
        )[0]
        self.assertEqual(
            hashlib.sha256(original_dispatch.encode()).hexdigest(),
            "7a803c8850284ef0fa392b89be84883da3a74da6a64cd4c8100ab9cec77062c6",
        )


if __name__ == "__main__":
    unittest.main()
