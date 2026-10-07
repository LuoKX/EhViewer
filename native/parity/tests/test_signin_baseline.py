"""Source-named SignIn contract checks; temporary writes stay in parity workspace."""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
SOURCE_REPO = Path(
    os.environ.get("EHVIEWER_SOURCE_REPO", str(WORKSPACE / "../.."))
).resolve()
FIXTURES = WORKSPACE / "fixtures/signin"
spec = importlib.util.spec_from_file_location(
    "export_signin_baseline", WORKSPACE / "tools/export_signin_baseline.py"
)
if spec is None or spec.loader is None:
    raise RuntimeError("Cannot load SignIn baseline exporter")
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)


class SignInBaselineTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.baseline = json.loads(
            (FIXTURES / "baseline.json").read_text(encoding="utf-8")
        )
        if not (SOURCE_REPO / exporter.SIGNIN).is_file():
            raise RuntimeError(
                "Set EHVIEWER_SOURCE_REPO to canonical EhViewer source root"
            )

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(
            prefix="signin-test-", dir=WORKSPACE
        )
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name) / "repo"
        self.output = Path(self.temporary.name) / "output"
        for source in self.baseline["sources"]:
            destination = self.repo / source["path"]
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(SOURCE_REPO / source["path"], destination)
        (self.repo / ".git").mkdir()
        (self.repo / ".git/HEAD").write_text(exporter.EXPECTED_HEAD + "\n")

    def build(self):
        return json.loads(exporter.build_export(self.repo)["baseline.json"])

    def replace_source(self, path, before, after):
        file = self.repo / path
        content = file.read_text(encoding="utf-8")
        self.assertIn(before, content)
        file.write_text(content.replace(before, after, 1), encoding="utf-8")

    def test_canonical_working_tree_export_matches_checked_fixture(self):
        self.assertEqual(
            exporter.build_export(SOURCE_REPO),
            {
                name: (FIXTURES / name).read_bytes()
                for name in (
                    "baseline.json",
                    "assets/ic_launcher_foreground.xml",
                    "assets/LICENSE",
                )
            },
        )

    def test_export_deterministic_and_path_independent(self):
        first = exporter.build_export(self.repo)
        second = exporter.build_export(self.repo)
        self.assertEqual(first, second)
        self.assertEqual(
            first["baseline.json"], (FIXTURES / "baseline.json").read_bytes()
        )
        self.assertNotIn(str(self.repo).encode(), first["baseline.json"])

    def test_window_size_class_expanded_inclusive_symbolic_threshold(self):
        expanded = self.baseline["layout"]["expanded"]
        self.assertEqual(
            expanded["predicate"],
            "windowSizeClass.isWidthAtLeastBreakpoint(WIDTH_DP_EXPANDED_LOWER_BOUND)",
        )
        self.assertTrue(expanded["inclusive"])
        self.assertIsNone(expanded["numeric_dp"])
        self.assertIn("numeric definition absent", expanded["unresolved"])
        self.assertIn("compact and medium", expanded["non_expanded_branch"])

    def test_sign_in_screen_ordered_username_password_validation(self):
        steps = self.baseline["interactions"]["validation_steps"]
        self.assertEqual(steps[0]["condition"], "signInJob?.isActive == true")
        self.assertEqual(
            [step["field"] for step in steps if "field" in step],
            ["username", "password"],
        )
        self.assertEqual(steps[1]["condition"], "username.text.isEmpty()")
        self.assertIn("password state untouched", steps[1]["on_empty"])
        self.assertEqual(steps[2]["condition"], "password.text.isEmpty()")
        self.assertIn("No trim", self.baseline["interactions"]["validation_note"])
        source = (self.repo / exporter.SIGNIN).read_text()
        start = source.index("fun signIn()")
        self.assertLess(
            source.index("username.text.isEmpty()", start),
            source.index("password.text.isEmpty()", start),
        )
        self.assertLess(
            source.index("password.text.isEmpty()", start),
            source.index("isProgressIndicatorVisible = true", start),
        )

    def test_sign_in_screen_compact_expanded_component_and_button_order(self):
        layout = self.baseline["layout"]
        compact = layout["compact"]
        expanded = layout["expanded_layout"]
        self.assertEqual(
            compact["component_order"],
            [
                "logo",
                "username",
                "password",
                "app_waring",
                "app_waring_2",
                "keyline_spacer",
                "primary_button_row",
                "secondary_button_row",
            ],
        )
        self.assertEqual(compact["primary_button_order"], ["register", "sign_in"])
        self.assertEqual(expanded["primary_button_order"], ["sign_in", "register"])
        self.assertEqual(expanded["column_order"], ["caption", "form"])
        self.assertEqual(
            expanded["caption_component_order"], ["logo", "app_waring", "app_waring_2"]
        )
        self.assertEqual(
            expanded["form_component_order"],
            [
                "username",
                "password",
                "8dp_spacer",
                "primary_button_row",
                "8dp_spacer",
                "secondary_button_row",
            ],
        )
        for branch in (compact, expanded):
            self.assertEqual(
                branch["secondary_button_order"], ["sign_in_via_webview", "guest_mode"]
            )

    def test_app_dimen_xml_exact_widths_margins_and_qualifier_overrides(self):
        configs = {
            c["qualifier"]: c for c in self.baseline["dimensions"]["configurations"]
        }
        base = configs["values"]["resources"]
        self.assertEqual(
            {name: base[name]["declarations"][0]["dp"] for name in exporter.DIMENSIONS},
            {
                "keyline_margin": 16,
                "single_max_width": 260,
                "signinscreen_landscape_caption_frame_width": 480,
            },
        )
        tablet = configs["values-sw600dp"]["resources"]
        self.assertEqual(tablet["keyline_margin"]["declarations"][0]["dp"], 24)
        self.assertEqual(tablet["single_max_width"]["declarations"][0]["dp"], 480)
        self.assertTrue(tablet["single_max_width"]["overrides_main_base"])
        self.assertEqual(
            tablet["signinscreen_landscape_caption_frame_width"]["status"],
            "missing_in_configuration",
        )
        self.assertEqual(
            configs["values-television"]["resources"][
                "signinscreen_landscape_caption_frame_width"
            ]["declarations"][0]["dp"],
            640,
        )
        self.assertTrue(
            all(
                r["status"] == "missing_in_configuration"
                for r in configs["values-sw720dp-land"]["resources"].values()
            )
        )
        layout = self.baseline["layout"]
        self.assertEqual(
            layout["shared"]["fields"]["width_resource"], "single_max_width"
        )
        self.assertEqual(layout["shared"]["fields"]["normal_bottom_padding_dp"], 16)
        self.assertEqual(layout["compact"]["warning_top_padding_dp"], [24, 12])
        self.assertEqual(layout["expanded_layout"]["warning_max_width_dp"], 360)
        self.assertEqual(layout["expanded_layout"]["button_width_dp"], 128)
        self.assertEqual(layout["expanded_layout"]["button_horizontal_padding_dp"], 4)
        self.assertIn("not device pixels", self.baseline["dimensions"]["unit"])

    def test_core_i18n_all_existing_locales_and_sign_in_keys(self):
        strings = self.baseline["strings"]
        self.assertEqual(
            list(strings["locales"]),
            [
                "base",
                "de",
                "es",
                "fr",
                "ja",
                "ko",
                "nb-rNO",
                "th",
                "tr",
                "zh-rCN",
                "zh-rHK",
                "zh-rTW",
            ],
        )
        self.assertEqual(set(strings["resource_keys"]), set(exporter.LABELS))
        for record in strings["locales"].values():
            self.assertEqual(set(record["resources"]), set(exporter.LABELS))
        self.assertEqual(
            strings["locales"]["nb-rNO"]["resources"]["app_waring"]["status"], "missing"
        )
        missing = strings["locales"]["de"]["resources"]["sign_in_failed_tip"]
        self.assertEqual(missing["status"], "missing")
        self.assertEqual(missing["declarations"], [])
        self.assertIn("/base/", missing["base_fallback_candidate"]["source"]["path"])
        declared = strings["locales"]["ja"]["resources"]["sign_in_failed_tip"]
        self.assertTrue(declared["overrides_base"])
        self.assertIsNone(declared["base_fallback_candidate"])

    def test_core_i18n_preserves_escape_and_format_markers(self):
        base_tip = self.baseline["strings"]["locales"]["base"]["resources"][
            "sign_in_failed_tip"
        ]["declarations"][0]
        self.assertEqual(
            base_tip["xml_inner_raw"], 'If this issue continues, try \\"%s\\".'
        )
        self.assertEqual(base_tip["xml_text"], base_tip["xml_inner_raw"])
        path = exporter.I18N + "/es/strings.xml"
        file = self.repo / path
        raw = "&quot;User&quot;\\n%1$s &amp; &lt;Guest&gt;"
        content = file.read_text()
        import re

        content, count = re.subn(
            r'(<string name="username">).*?(</string>)',
            lambda m: m[1] + raw + m[2],
            content,
            count=1,
            flags=re.DOTALL,
        )
        self.assertEqual(count, 1)
        file.write_text(content)
        value = self.build()["strings"]["locales"]["es"]["resources"]["username"][
            "declarations"
        ][0]
        self.assertEqual(value["xml_inner_raw"], raw)
        self.assertEqual(value["xml_text"], '"User"\\n%1$s & <Guest>')
        self.assertIn("%1$s", value["xml_element_raw"])

    def test_sign_in_failure_runtime_message_not_fabricated(self):
        failure = self.baseline["interactions"]["failure_dialog"]
        self.assertEqual(failure["title_resource"], "sign_in_failed")
        self.assertEqual(failure["confirm_resource"], "get_it")
        self.assertFalse(failure["showCancelButton"])
        self.assertIn("${it.displayString()}", failure["message_source"])
        self.assertIn(".trimIndent()", failure["message_source"])
        self.assertIn(
            "sign_in_failed_tip, stringResource(R.string.sign_in_via_webview)",
            failure["message_source"],
        )
        self.assertIn("CancellationException", failure["cancellation"])

    def test_theme_defaults_source_references_no_fixed_dynamic_palette(self):
        theme = self.baseline["theme"]
        self.assertEqual(theme["defaults"]["theme"]["default"], -1)
        self.assertEqual(theme["defaults"]["blackDarkTheme"]["default"], False)
        self.assertEqual(theme["defaults"]["harmonizeCategoryColor"]["default"], True)
        self.assertEqual(theme["defaults"]["theme"]["preference_key"], "theme_2")
        self.assertIn("isSystemInDarkTheme()", theme["dark_mode_input"])
        self.assertIn("dynamicDarkColorScheme", theme["color_branches"][0])
        self.assertIn("expressiveLightColorScheme", theme["color_branches"][2])
        self.assertIn("unresolved", theme["typography"])
        self.assertEqual(
            theme["source_text"][exporter.THEME],
            (self.repo / exporter.THEME).read_text(),
        )

    def test_launcher_foreground_logo_original_bytes_provenance_and_license(self):
        logo = self.baseline["logo"]
        source = (self.repo / exporter.LOGO).read_bytes()
        self.assertEqual(source, (FIXTURES / logo["asset"]).read_bytes())
        self.assertEqual(hashlib.sha256(source).hexdigest(), logo["sha256"])
        self.assertEqual(logo["vector"]["attributes"]["width"], "108dp")
        self.assertEqual(logo["vector"]["attributes"]["height"], "108dp")
        group = logo["vector"]["children"][0]
        self.assertEqual(group["attributes"]["scaleX"], "0.7425")
        self.assertEqual(group["attributes"]["translateX"], "30.68")
        self.assertEqual(len(group["children"]), 7)
        self.assertIn(b"Copyright 2022 Tarsin Norbin", source)
        self.assertEqual(logo["license"]["spdx"], "GPL-3.0-or-later")
        self.assertEqual(
            (self.repo / "LICENSE").read_bytes(),
            (FIXTURES / "assets/LICENSE").read_bytes(),
        )

    def test_export_and_check_never_modify_source_or_checked_output(self):
        before = {
            p.relative_to(self.repo): p.read_bytes()
            for p in self.repo.rglob("*")
            if p.is_file()
        }
        exporter.export_baseline(self.repo, self.output)
        generated_before = {
            p.relative_to(self.output): (p.read_bytes(), p.stat().st_mtime_ns)
            for p in self.output.rglob("*")
            if p.is_file()
        }
        exporter.export_baseline(self.repo, self.output, check=True)
        self.assertEqual(
            before,
            {
                p.relative_to(self.repo): p.read_bytes()
                for p in self.repo.rglob("*")
                if p.is_file()
            },
        )
        self.assertEqual(
            generated_before,
            {
                p.relative_to(self.output): (p.read_bytes(), p.stat().st_mtime_ns)
                for p in self.output.rglob("*")
                if p.is_file()
            },
        )

    def test_check_detects_xml_source_drift_without_rewriting_fixture(self):
        exporter.export_baseline(self.repo, self.output)
        before = (self.output / "baseline.json").read_bytes()
        self.replace_source("app/src/main/res/values/dimens.xml", "260dp", "261dp")
        with self.assertRaisesRegex(
            exporter.BaselineError, "Baseline drift: baseline.json"
        ):
            exporter.export_baseline(self.repo, self.output, check=True)
        self.assertEqual((self.output / "baseline.json").read_bytes(), before)
        self.assertEqual(
            self.build()["dimensions"]["configurations"][0]["resources"][
                "single_max_width"
            ]["declarations"][0]["dp"],
            261,
        )

    def test_check_detects_asset_tamper_and_missing_output(self):
        exporter.export_baseline(self.repo, self.output)
        (self.output / "assets/ic_launcher_foreground.xml").write_bytes(b"tampered")
        (self.output / "assets/LICENSE").unlink()
        with self.assertRaises(exporter.BaselineError) as error:
            exporter.export_baseline(self.repo, self.output, check=True)
        self.assertIn(
            "Baseline drift: assets/ic_launcher_foreground.xml", str(error.exception)
        )
        self.assertIn("Missing baseline artifact: assets/LICENSE", str(error.exception))
        self.assertEqual(
            (self.output / "assets/ic_launcher_foreground.xml").read_bytes(),
            b"tampered",
        )
        self.assertFalse((self.output / "assets/LICENSE").exists())

    def test_sign_in_screen_unknown_validation_order_fails_closed(self):
        self.replace_source(
            exporter.SIGNIN,
            "if (username.text.isEmpty())",
            "if (password.text.isEmpty())",
        )
        with self.assertRaisesRegex(
            exporter.BaselineError, "Unsupported Kotlin source drift: .*SignInScreen.kt"
        ):
            exporter.build_export(self.repo)
        self.assertFalse(self.output.exists())

    def test_sign_in_screen_unknown_button_order_fails_closed(self):
        self.replace_source(exporter.SIGNIN, "R.string.register", "R.string.sign_in")
        with self.assertRaisesRegex(
            exporter.BaselineError, "Unsupported Kotlin source drift"
        ):
            exporter.build_export(self.repo)

    def test_theme_unknown_source_shape_fails_closed(self):
        self.replace_source(
            exporter.THEME, "expressiveLightColorScheme()", "lightColorScheme()"
        )
        with self.assertRaisesRegex(
            exporter.BaselineError, "Unsupported Kotlin source drift"
        ):
            exporter.build_export(self.repo)

    def test_missing_base_resource_fails_closed(self):
        self.replace_source(
            exporter.I18N + "/base/strings.xml",
            '<string name="username">Username</string>',
            "",
        )
        with self.assertRaisesRegex(
            exporter.BaselineError, "Missing or ambiguous base SignIn string"
        ):
            exporter.build_export(self.repo)

    def test_missing_required_source_fails_closed(self):
        (self.repo / exporter.LOGO).unlink()
        with self.assertRaisesRegex(exporter.BaselineError, "Missing canonical source"):
            exporter.build_export(self.repo)

    def test_new_locale_missing_labels_explicit_not_guessed(self):
        file = self.repo / exporter.I18N / "it/strings.xml"
        file.parent.mkdir()
        file.write_text(
            '<resources><string name="username">Utente</string></resources>'
        )
        locale = self.build()["strings"]["locales"]["it"]["resources"]
        self.assertEqual(
            locale["username"]["declarations"][0]["xml_inner_raw"], "Utente"
        )
        self.assertEqual(locale["password"]["status"], "missing")
        self.assertEqual(locale["password"]["declarations"], [])
        self.assertEqual(
            locale["password"]["base_fallback_candidate"]["xml_inner_raw"], "Password"
        )

    def test_locale_conflicts_explicit_no_arbitrary_override_choice(self):
        file = self.repo / exporter.I18N / "de/extra.xml"
        file.write_text(
            '<resources><string name="username">Override</string></resources>'
        )
        value = self.build()["strings"]["locales"]["de"]["resources"]["username"]
        self.assertEqual(value["status"], "ambiguous_multiple_declarations")
        self.assertEqual(len(value["declarations"]), 2)
        self.assertIsNone(value["base_fallback_candidate"])

    def test_dimen_in_other_filename_and_source_set_override_explicit(self):
        file = self.repo / "app/src/debug/res/values-land/custom.xml"
        file.parent.mkdir(parents=True)
        file.write_text(
            '<resources><dimen name="single_max_width">900dp</dimen></resources>'
        )
        configs = self.build()["dimensions"]["configurations"]
        config = next(c for c in configs if c["source_set"] == "debug")
        value = config["resources"]["single_max_width"]
        self.assertEqual(config["qualifier"], "values-land")
        self.assertTrue(value["overrides_main_base"])
        self.assertEqual(value["declarations"][0]["dp"], 900)

    def test_unqualified_source_set_override_explicit(self):
        file = self.repo / "app/src/debug/res/values/custom.xml"
        file.parent.mkdir(parents=True)
        file.write_text(
            '<resources><dimen name="single_max_width">700dp</dimen></resources>'
        )
        config = next(
            c
            for c in self.build()["dimensions"]["configurations"]
            if c["source_set"] == "debug"
        )
        self.assertTrue(config["resources"]["single_max_width"]["overrides_main_base"])

    def test_dimen_alias_unresolved_and_conflicts_explicit(self):
        file = self.repo / "app/src/main/res/values-sw600dp/extra.xml"
        file.write_text(
            '<resources><dimen name="single_max_width">@dimen/keyline_margin</dimen></resources>'
        )
        config = next(
            c
            for c in self.build()["dimensions"]["configurations"]
            if c["qualifier"] == "values-sw600dp"
        )
        value = config["resources"]["single_max_width"]
        self.assertEqual(value["status"], "ambiguous_multiple_declarations")
        alias = next(d for d in value["declarations"] if d["value_raw"].startswith("@"))
        self.assertIsNone(alias["dp"])
        self.assertEqual(alias["value_kind"], "alias_or_unresolved")

    def test_resource_xml_rejects_dtd_entities_and_oversize(self):
        for text in [
            '<!DOCTYPE resources [<!ENTITY x "expanded">]><resources>&x;</resources>',
            '<!ENTITY x SYSTEM "file:///etc/passwd"><resources/>',
        ]:
            with (
                self.subTest(text=text),
                self.assertRaisesRegex(exporter.BaselineError, "DTD/entity"),
            ):
                exporter.resource_xml(text)
        with self.assertRaisesRegex(exporter.BaselineError, "8 MiB"):
            exporter.resource_xml(
                "<resources>" + (" " * (8 * 1024 * 1024)) + "</resources>"
            )
        self.assertEqual(exporter.resource_xml("<resources/>").tag, "resources")

    def test_unknown_item_resource_shape_fails_closed(self):
        file = self.repo / "app/src/main/res/values/custom.xml"
        file.write_text(
            '<resources><item type="dimen" name="single_max_width">999dp</item></resources>'
        )
        with self.assertRaisesRegex(
            exporter.BaselineError, "Unsupported item resource declaration"
        ):
            exporter.build_export(self.repo)

    def test_logo_license_notice_drift_fails_closed(self):
        self.replace_source(
            exporter.LOGO, "Copyright 2022 Tarsin Norbin", "Copyright Other"
        )
        with self.assertRaisesRegex(
            exporter.BaselineError, "Unsupported canonical logo license notice"
        ):
            exporter.build_export(self.repo)

    def test_source_inventory_hashes_and_no_git_metadata_required(self):
        for source in self.baseline["sources"]:
            data = (self.repo / source["path"]).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), source["sha256"])
            self.assertEqual(len(data), source["size_bytes"])
        shutil.rmtree(self.repo / ".git")
        self.assertNotIn("observed_head", self.build()["provenance"])
        self.assertIn("working_tree_bytes", self.baseline["provenance"]["source_state"])

    def test_git_head_advance_does_not_change_source_bound_fixture(self):
        before = exporter.build_export(self.repo)
        (self.repo / ".git/HEAD").write_text(("1" * 40) + "\n")
        self.assertEqual(exporter.build_export(self.repo), before)
        self.assertEqual(exporter.read_head(self.repo), "1" * 40)
        self.assertNotIn("observed_head", self.build()["provenance"])

    def test_cli_check_and_verify_alias_return_nonzero_on_drift(self):
        arguments = ["--repo", str(self.repo), "--output", str(self.output)]
        with (
            contextlib.redirect_stdout(io.StringIO()),
            contextlib.redirect_stderr(io.StringIO()) as errors,
        ):
            self.assertEqual(exporter.main(arguments), 0)
            self.assertEqual(exporter.main(arguments + ["--check"]), 0)
            self.assertEqual(exporter.main(arguments + ["--verify"]), 0)
            (self.output / "baseline.json").write_bytes(b"stale")
            self.assertEqual(exporter.main(arguments + ["--check"]), 1)
        self.assertIn("Baseline drift", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
