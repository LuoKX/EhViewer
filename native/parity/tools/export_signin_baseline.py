#!/usr/bin/env python3
"""Export checked canonical SignIn source facts, never a rendering-parity claim.

Only Python stdlib. --output is a directory; --check/--verify never writes.
Kotlin facts below support one reviewed source shape. Unknown Kotlin bytes fail
closed; updating SUPPORTED_KOTLIN requires reviewing facts against new source.
XML dimensions/strings are read directly, including overrides and missing keys.
"""

import argparse
import hashlib
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

SIGNIN = "app/src/main/kotlin/com/hippo/ehviewer/ui/login/SignInScreen.kt"
WINDOW = "core/ui/src/commonMain/kotlin/com/ehviewer/core/ui/util/WindowSizeClass.kt"
THEME = "app/src/main/kotlin/com/hippo/ehviewer/ui/theme/Theme.kt"
UI_THEME = "app/src/main/kotlin/com/hippo/ehviewer/ui/Theme.kt"
SETTINGS = "app/src/main/kotlin/com/hippo/ehviewer/Settings.kt"
COLLECTOR = "app/src/main/kotlin/com/hippo/ehviewer/SettingsCollector.kt"
URLS = "app/src/main/kotlin/com/hippo/ehviewer/client/EhUrl.kt"
LOGO = "app/src/main/res/drawable/ic_launcher_foreground.xml"
I18N = "core/i18n/src/commonMain/moko-resources"
DIMENSIONS = (
    "keyline_margin",
    "single_max_width",
    "signinscreen_landscape_caption_frame_width",
)
LABELS = (
    "username",
    "password",
    "app_waring",
    "app_waring_2",
    "error_username_cannot_empty",
    "error_password_cannot_empty",
    "register",
    "sign_in",
    "guest_mode",
    "sign_in_via_webview",
    "sign_in_failed",
    "sign_in_failed_tip",
    "get_it",
)
SUPPORTED_KOTLIN = {
    SIGNIN: "c295b63ed934e8b12c11314e757bad7eda42f96716f2a2965abe4199fbe7e689",
    WINDOW: "836482fba054507f85248def6415ef8f0960d09b223001fac76bee8d493b5bee",
    THEME: "5a51d2ecc055b1bf556e7bbed3aff9f4f6d4f3b630d967c6192314bc5d5f5e2b",
    UI_THEME: "0e06ce3b3af4a60b16048b4d87b181519ad8817e92ab6c2667bafc1feab331aa",
    COLLECTOR: "b26f215334eb65625985c0c6fc791d1927198729bc9c68ffbba434510d6bbcbf",
}
EXPECTED_HEAD = "cd07574903bdf83e79f28975359c34ef98908ad6"


class BaselineError(ValueError):
    pass


def resource_xml(text):
    """Parse bounded repository XML; reject DTD/entity declarations before expansion."""
    if len(text.encode("utf-8")) > 8 * 1024 * 1024:
        raise BaselineError("Repository XML exceeds 8 MiB inspection budget")
    if re.search(r"<!\s*(?:DOCTYPE|ENTITY)\b", text, re.IGNORECASE):
        raise BaselineError("DTD/entity declarations are not supported in resource XML")
    # Repository XML only; explicit size/DTD/entity checks above prevent entity expansion.
    return ET.fromstring(text)  # noqa: S314


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_head(repo):
    """Read Git metadata without invoking Git or altering index/worktree."""
    git = repo / ".git"
    if git.is_file():
        value = git.read_text(encoding="utf-8").strip()
        if not value.startswith("gitdir: "):
            raise BaselineError("Unsupported .git file")
        git = (repo / value[8:]).resolve()
    if not (git / "HEAD").is_file():
        return None
    head = (git / "HEAD").read_text(encoding="utf-8").strip()
    if head.startswith("ref: "):
        ref = head[5:]
        common = git
        if (git / "commondir").is_file():
            common = (git / (git / "commondir").read_text().strip()).resolve()
        for directory in (git, common):
            if (directory / ref).is_file():
                head = (directory / ref).read_text().strip()
                break
        else:
            packed = common / "packed-refs"
            refs = (
                {}
                if not packed.is_file()
                else {
                    line.split()[1]: line.split()[0]
                    for line in packed.read_text().splitlines()
                    if line and not line.startswith(("#", "^"))
                }
            )
            head = refs.get(ref, "")
    if not re.fullmatch(r"[0-9a-f]{40,64}", head):
        raise BaselineError("Cannot resolve source repository HEAD")
    return head


class Sources:
    def __init__(self, repo):
        self.repo = Path(repo).resolve()
        self.data = {}

    def read(self, path):
        if path not in self.data:
            target = self.repo / path
            if not target.is_file():
                raise BaselineError("Missing canonical source: " + path)
            self.data[path] = target.read_bytes()
        return self.data[path]

    def text(self, path):
        return self.read(path).decode("utf-8")

    def ref(self, path, needle=None):
        result = {"path": path}
        if needle is not None:
            text = self.text(path)
            if needle not in text:
                raise BaselineError(
                    f"Missing expected source anchor {needle!r} in {path}"
                )
            result["line"] = text[: text.index(needle)].count("\n") + 1
            result["anchor"] = needle
        return result

    def inventory(self):
        return [
            {"path": path, "sha256": sha256(data), "size_bytes": len(data)}
            for path, data in sorted(self.data.items())
        ]


def xml_declarations(sources, path, tag, names):
    """Keep exact XML inner/element bytes plus XML-decoded (not runtime) text."""
    text = sources.text(path)
    root = resource_xml(text)
    if root.tag != "resources":
        raise BaselineError("Expected resources XML: " + path)
    if any(
        node.tag == "item"
        and node.attrib.get("type") == tag
        and node.attrib.get("name") in names
        for node in root
    ):
        raise BaselineError("Unsupported item resource declaration: " + path)
    # Comments cannot become declarations; retain offsets for source line refs.
    uncommented = re.sub(
        r"<!--.*?-->",
        lambda m: "".join("\n" if c == "\n" else " " for c in m.group()),
        text,
        flags=re.DOTALL,
    )
    pattern = re.compile(
        r"<" + tag + r"\b[^>]*?(?:/>|>.*?</" + tag + r"\s*>)", re.DOTALL
    )
    matches = list(pattern.finditer(uncommented))
    elements = [node for node in root if node.tag == tag]
    if len(matches) != len(elements):
        raise BaselineError("Unsupported XML declaration shape: " + path)
    result = {name: [] for name in names}
    for node, match in zip(elements, matches, strict=True):
        name = node.attrib.get("name")
        if name not in result:
            continue
        raw = text[match.start() : match.end()]
        opening_end = raw.index(">")
        inner = (
            ""
            if raw[:opening_end].rstrip().endswith("/")
            else raw[opening_end + 1 : raw.rfind("</")]
        )
        declaration = {
            "source": {"path": path, "line": text[: match.start()].count("\n") + 1},
            "attributes": dict(sorted(node.attrib.items())),
            "xml_element_raw": raw,
            "xml_inner_raw": inner,
            "xml_text": "".join(node.itertext()),
        }
        if tag == "dimen":
            value = declaration["xml_text"].strip()
            literal = re.fullmatch(r"(-?\d+(?:\.\d+)?)dp", value)
            declaration.update(
                value_raw=value,
                dp=float(literal[1]) if literal else None,
                value_kind="dp_literal" if literal else "alias_or_unresolved",
            )
        result[name].append(declaration)
    return result


def export_dimensions(sources):
    configurations = []
    for directory in sorted((sources.repo / "app/src").glob("*/res/values*")):
        if not directory.is_dir():
            continue
        declarations = {name: [] for name in DIMENSIONS}
        inspected = []
        for file in sorted(directory.glob("*.xml")):
            path = file.relative_to(sources.repo).as_posix()
            # Parse all values XML: dimensions can be declared in any filename.
            found = xml_declarations(sources, path, "dimen", DIMENSIONS)
            if any(found.values()) or file.name.startswith("dimen"):
                inspected.append(path)
                for name in DIMENSIONS:
                    declarations[name].extend(found[name])
            else:
                # Non-dimension XML is outside this seam. Discovery runs again on check.
                del sources.data[path]
        if not inspected:
            continue
        configurations.append(
            {
                "source_set": directory.parent.parent.name,
                "qualifier": directory.name,
                "files": inspected,
                "resources": {
                    name: {
                        "status": "missing_in_configuration"
                        if not declarations[name]
                        else "declared"
                        if len(declarations[name]) == 1
                        else "ambiguous_multiple_declarations",
                        "overrides_main_base": (
                            directory.parent.parent.name != "main"
                            or directory.name != "values"
                        )
                        and bool(declarations[name]),
                        "declarations": declarations[name],
                    }
                    for name in DIMENSIONS
                },
            }
        )
    base = next(
        (
            c
            for c in configurations
            if c["source_set"] == "main" and c["qualifier"] == "values"
        ),
        None,
    )
    if base is None or any(
        base["resources"][n]["status"] != "declared" for n in DIMENSIONS
    ):
        raise BaselineError("Missing or ambiguous main/values SignIn dimensions")
    return {
        "unit": "dp (logical density-independent units, not device pixels)",
        "configurations": configurations,
        "selection": "Android qualifier/source-set precedence not resolved for a device. Missing entries are explicit; do not treat sw600dp as expanded width breakpoint.",
    }


def export_strings(sources):
    root = sources.repo / I18N
    if not root.is_dir():
        raise BaselineError("Missing canonical i18n directory")
    locales = {}
    for directory in sorted(root.iterdir()):
        if not directory.is_dir():
            continue
        declarations = {name: [] for name in LABELS}
        files = []
        for file in sorted(directory.glob("*.xml")):
            path = file.relative_to(sources.repo).as_posix()
            files.append(path)
            found = xml_declarations(sources, path, "string", LABELS)
            for name in LABELS:
                declarations[name].extend(found[name])
        locales[directory.name] = {
            "files": files,
            "resources": {
                name: {
                    "status": "missing"
                    if not declarations[name]
                    else "declared"
                    if len(declarations[name]) == 1
                    else "ambiguous_multiple_declarations",
                    "declarations": declarations[name],
                }
                for name in LABELS
            },
        }
    base = locales.get("base")
    if base is None or any(
        base["resources"][name]["status"] != "declared" for name in LABELS
    ):
        raise BaselineError("Missing or ambiguous base SignIn string")
    for locale, record in locales.items():
        for name, entry in record["resources"].items():
            entry["overrides_base"] = locale != "base" and bool(entry["declarations"])
            entry["base_fallback_candidate"] = (
                base["resources"][name]["declarations"][0]
                if entry["status"] == "missing"
                else None
            )
    return {
        "resource_keys": list(LABELS),
        "encoding": "UTF-8; xml_inner_raw/xml_element_raw preserve XML entities, backslashes, markup and format markers. xml_text only decodes XML, not Android/Moko escapes or formatting.",
        "fallback": "Missing local definitions have explicit base_fallback_candidate; generated Android/Moko locale selection and escaping are not verified here. Ambiguous definitions have no chosen value.",
        "locales": locales,
    }


def layout_facts(sources):
    def ref(needle):
        return sources.ref(SIGNIN, needle)

    return {
        "source": sources.ref(SIGNIN),
        "expanded": {
            "source": sources.ref(
                WINDOW, "isWidthAtLeastBreakpoint(WIDTH_DP_EXPANDED_LOWER_BOUND)"
            ),
            "predicate": "windowSizeClass.isWidthAtLeastBreakpoint(WIDTH_DP_EXPANDED_LOWER_BOUND)",
            "inclusive": True,
            "axis": "window width dp; not orientation, height, or smallest-width resource qualifier",
            "numeric_dp": None,
            "unresolved": "WIDTH_DP_EXPANDED_LOWER_BOUND belongs to androidx.window.core.layout.WindowSizeClass; numeric definition absent from canonical source. Resolve current dependency with separate provenance before implementing Rust breakpoint.",
            "non_expanded_branch": "!windowSizeClass.isExpanded (includes both compact and medium AndroidX classes)",
        },
        "shared": {
            "outer": [
                "Box(contentAlignment = Alignment.Center)",
                "fillMaxSize",
                "verticalScroll",
                "systemBarsPadding",
                "padding(@dimen/keyline_margin)",
            ],
            "fields": {
                "order": ["username", "password"],
                "width_resource": "single_max_width",
                "width_operator": "Modifier.width (preferred width subject to parent constraints)",
                "normal_bottom_padding_dp": 16,
                "error_bottom_padding": "normal 16dp padding omitted when respective error shown; supportingText rendered instead",
                "source": ref("fun UsernameAndPasswordTextField()"),
            },
            "logo_padding_resource": "keyline_margin",
            "warnings_typography": {
                "app_waring": "MaterialTheme.typography.titleMedium",
                "app_waring_2": "MaterialTheme.typography.titleLarge",
            },
            "button_shapes": "ButtonDefaults.shapes() (Material3 dependency supplied)",
            "progress_overlay": "CircularWavyProgressIndicator at Box center when isProgressIndicatorVisible",
        },
        "compact": {
            "source": ref("!windowSizeClass.isExpanded ->"),
            "container": "Column; horizontalAlignment = Alignment.CenterHorizontally",
            "component_order": [
                "logo",
                "username",
                "password",
                "app_waring",
                "app_waring_2",
                "keyline_spacer",
                "primary_button_row",
                "secondary_button_row",
            ],
            "primary_button_order": ["register", "sign_in"],
            "secondary_button_order": ["sign_in_via_webview", "guest_mode"],
            "warning_max_width_resource": "single_max_width",
            "warning_top_padding_dp": [24, 12],
            "spacer_height_resource": "keyline_margin",
            "primary_row_top_padding_resource": "keyline_margin",
            "primary_buttons": {"weight_each": 1, "horizontal_padding_dp": 4},
            "secondary_row": {
                "horizontal_padding_dp": 4,
                "spacing_dp": 8,
                "weight_each": 1,
                "text_decoration": "Underline",
            },
        },
        "expanded_layout": {
            "source": ref("signinscreen_landscape_caption_frame_width"),
            "container": "Row; verticalAlignment = Alignment.CenterVertically",
            "column_order": ["caption", "form"],
            "caption_component_order": ["logo", "app_waring", "app_waring_2"],
            "caption_width_resource": "signinscreen_landscape_caption_frame_width",
            "caption_padding_resource": "keyline_margin",
            "warning_max_width_dp": 360,
            "warning_added_top_padding_dp": [0, 0],
            "form": "Column.fillMaxWidth; horizontalAlignment = Alignment.CenterHorizontally",
            "form_component_order": [
                "username",
                "password",
                "8dp_spacer",
                "primary_button_row",
                "8dp_spacer",
                "secondary_button_row",
            ],
            "primary_button_order": ["sign_in", "register"],
            "secondary_button_order": ["sign_in_via_webview", "guest_mode"],
            "button_width_dp": 128,
            "button_horizontal_padding_dp": 4,
            "button_modifier_order": ["padding(horizontal = 4.dp)", "width(128.dp)"],
            "row_arrangement": "Arrangement.Center",
            "secondary_text_decoration": "Underline",
        },
        "button_roles": {
            "register": "FilledTonalButton",
            "sign_in": "Button",
            "sign_in_via_webview": "TextButton",
            "guest_mode": "TextButton",
        },
        "inline_dp_occurrences": [
            {
                "dp": float(m[1]),
                "source": {
                    "path": SIGNIN,
                    "line": sources.text(SIGNIN)[: m.start()].count("\n") + 1,
                },
            }
            for m in re.finditer(r"(\d+(?:\.\d+)?)\.dp", sources.text(SIGNIN))
        ],
    }


def interaction_facts(sources):
    url_match = re.search(r'const val URL_REGISTER = "([^"\n]+)"', sources.text(URLS))
    if not url_match:
        raise BaselineError("Cannot extract canonical URL_REGISTER literal")
    return {
        "source": sources.ref(SIGNIN, "fun signIn()"),
        "initial_state": {
            "isProgressIndicatorVisible": False,
            "showUsernameError": False,
            "showPasswordError": False,
            "passwordHidden": True,
            "username": "",
            "password": "",
            "signInJob": None,
        },
        "validation_steps": [
            {
                "condition": "signInJob?.isActive == true",
                "action": "return; no validation/state changes",
            },
            {
                "field": "username",
                "condition": "username.text.isEmpty()",
                "on_empty": "showUsernameError = true; return; password state untouched",
                "otherwise": "showUsernameError = false",
            },
            {
                "field": "password",
                "condition": "password.text.isEmpty()",
                "on_empty": "showPasswordError = true; return",
                "otherwise": "showPasswordError = false",
            },
            {
                "action": "focusManager.clearFocus(); isProgressIndicatorVisible = true; launchIO sign-in job"
            },
        ],
        "validation_note": "No trim/whitespace rejection; username error returns before password check. Error reset only on next submit reaching that field, not on edit.",
        "username": {
            "widget": "OutlinedTextField",
            "autofill": "ContentType.Username",
            "ime_action": "ImeAction.Next",
            "line_limits": "TextFieldLineLimits.SingleLine",
            "error_icon": "Icons.Filled.Info",
        },
        "password": {
            "widget": "OutlinedSecureTextField",
            "autofill": "ContentType.Password",
            "keyboard_type": "KeyboardType.Password",
            "keyboard_action": "signIn()",
            "error_icon": "Icons.Filled.Info (replaces visibility button)",
            "hidden_mode": "TextObfuscationMode.RevealLastTyped",
            "visible_mode": "TextObfuscationMode.Visible",
            "toggle": "passwordHidden = !passwordHidden",
            "toggle_icons": ["Icons.Filled.Visibility", "Icons.Filled.VisibilityOff"],
            "icon_content_descriptions": None,
        },
        "actions": {
            "register": {
                "call": "openBrowser(EhUrl.URL_REGISTER)",
                "url": url_match[1],
                "source": sources.ref(URLS, "const val URL_REGISTER"),
            },
            "webview": "navigate(WebViewSignInScreenDestination)",
            "guest": "active signInJob guard then launchIO { sessionController.enterGuest() }",
            "sign_in": "sessionController.beginSignIn(); EhEngine.signIn(username.text.toString(), password.text.toString()); postLogin(attempt, signedInUsername)",
        },
        "failure_dialog": {
            "title_resource": "sign_in_failed",
            "confirm_resource": "get_it",
            "showCancelButton": False,
            "message_parts": [
                "it.displayString() (runtime exception display; not fixed translation)",
                "stringResource(R.string.sign_in_failed_tip, stringResource(R.string.sign_in_via_webview))",
            ],
            "message_source": sources.text(SIGNIN)[
                sources.text(SIGNIN).index('"""') : sources.text(SIGNIN).index(
                    ".trimIndent()"
                )
                + len(".trimIndent()")
            ],
            "before_dialog": "clearFocus(); isProgressIndicatorVisible = false",
            "cancellation": "CancellationException rethrown; failure dialog not shown",
            "post_login_false": "isProgressIndicatorVisible = false",
        },
    }


def theme_facts(sources):
    defaults = {}
    for name in ("theme", "blackDarkTheme", "harmonizeCategoryColor"):
        match = re.search(
            r"val "
            + name
            + r' = (intPref|boolPref)\("([^"\n]+)", (-?\d+|true|false)\)',
            sources.text(SETTINGS),
        )
        if not match:
            raise BaselineError("Unsupported theme setting declaration: " + name)
        raw = match[3]
        defaults[name] = {
            "preference_key": match[2],
            "default": {"true": True, "false": False}.get(
                raw, int(raw) if re.fullmatch(r"-?\d+", raw) else raw
            ),
            "source": sources.ref(SETTINGS, "val " + name + " ="),
        }
    return {
        "defaults": defaults,
        "sources": [
            sources.ref(THEME),
            sources.ref(UI_THEME),
            sources.ref(COLLECTOR, "suspend fun updateWhenThemeChanges"),
        ],
        "source_text": {
            path: sources.text(path) for path in (THEME, UI_THEME, COLLECTOR)
        },
        "dark_mode_input": "setMD3Content: EhTheme(useDarkTheme = isSystemInDarkTheme())",
        "theme_setting_update": "delay(100); Android S+: map MODE_NIGHT_NO/YES to UiModeManager; else MODE_NIGHT_AUTO; AppCompatDelegate.setDefaultNightMode(theme)",
        "color_branches": [
            "Android S+: dynamicDarkColorScheme(context).amoled(amoled) or dynamicLightColorScheme(context)",
            "Before S and at least O_MR1: system wallpaper primary/secondary/tertiary to materialkolor.dynamicColorScheme when wallpaper palette available",
            "No wallpaper palette: darkColorScheme().amoled(amoled) or expressiveLightColorScheme()",
        ],
        "amoled_override": {
            "surface": "Color.Black",
            "onSurface": "Color.White",
            "background": "Color.Black",
            "onBackground": "Color.White",
        },
        "motion": "CustomMotionScheme delegates MotionScheme.expressive(); defaultSpatialSpec() = defaultEffectsSpec<T>()",
        "typography": "Not explicitly passed to MaterialTheme; dependency defaults unresolved",
        "runtime_values": "Wallpaper, OS API, system theme, library tokens, fonts and motion specs not captured by source-only baseline",
    }


def vector_tree(node):
    return {
        "tag": node.tag.rsplit("}", 1)[-1],
        "attributes": {
            key.rsplit("}", 1)[-1]: value for key, value in sorted(node.attrib.items())
        },
        "children": [vector_tree(child) for child in node],
    }


def build_export(repo):
    sources = Sources(repo)
    for path, expected_hash in SUPPORTED_KOTLIN.items():
        if sha256(sources.read(path)) != expected_hash:
            raise BaselineError(
                "Unsupported Kotlin source drift: "
                + path
                + "; review source facts before updating SUPPORTED_KOTLIN"
            )
    for path in (
        "gradle/libs.versions.toml",
        "core/ui/build.gradle.kts",
        "core/i18n/build.gradle.kts",
    ):
        sources.read(path)
    dimensions = export_dimensions(sources)
    strings = export_strings(sources)
    layout = layout_facts(sources)
    interactions = interaction_facts(sources)
    theme = theme_facts(sources)
    variants = sorted(
        p.relative_to(sources.repo).as_posix()
        for p in (sources.repo / "app/src").glob("*/res/*/ic_launcher_foreground.*")
    )
    for path in variants:
        sources.read(path)
    logo_bytes = sources.read(LOGO)
    license_bytes = sources.read("LICENSE")
    if not all(
        notice in logo_bytes
        for notice in (
            b"Copyright 2022 Tarsin Norbin",
            b"either version 3",
            b"any later version",
        )
    ):
        raise BaselineError(
            "Unsupported canonical logo license notice; review provenance"
        )
    if (
        b"GNU GENERAL PUBLIC LICENSE" not in license_bytes
        or b"Version 3, 29 June 2007" not in license_bytes
    ):
        raise BaselineError(
            "Unsupported canonical repository license; review provenance"
        )
    baseline = {
        "schema_version": 1,
        "scope": "Canonical working-tree SignIn presentation/interaction source baseline; not a Kotlin runtime bridge or renderer redesign",
        "provenance": {
            "expected_original_head": EXPECTED_HEAD,
            "head_policy": "Reference revision is historical; live Git HEAD is reported by CLI only, never embedded in deterministic artifacts",
            "source_state": "working_tree_bytes (existing user edits included; not claimed clean HEAD tree)",
        },
        "layout": layout,
        "dimensions": dimensions,
        "strings": strings,
        "interactions": interactions,
        "theme": theme,
        "logo": {
            "source": sources.ref(LOGO),
            "asset": "assets/ic_launcher_foreground.xml",
            "sha256": sha256(logo_bytes),
            "vector": vector_tree(resource_xml(logo_bytes.decode("utf-8"))),
            "variants": [
                {
                    "source": sources.ref(path),
                    "sha256": sha256(sources.read(path)),
                    "is_canonical_main_drawable": path == LOGO,
                }
                for path in variants
            ],
            "license": {
                "spdx": "GPL-3.0-or-later",
                "copyright": "Copyright 2022 Tarsin Norbin",
                "notice": "Original XML copyright/license notice retained byte-for-byte; no artwork conversion",
                "source": sources.ref("LICENSE"),
                "asset": "assets/LICENSE",
                "sha256": sha256(license_bytes),
            },
            "variant_selection": "Canonical main drawable exported; listed variant/source-set overrides require explicit Android resource selection if present",
        },
        "exact_parity_not_established": [
            "Numeric expanded lower bound requires dependency definition with provenance",
            "Android resource/locale/source-set resolution and Moko/Android runtime text escapes/formatting",
            "Material3 expressive widget shapes, typography, interaction states and motion tokens",
            "OS/wallpaper-dependent dynamic colors and system dark-mode preference behavior",
            "IME, autofill, accessibility, focus/scroll/insets and password reveal animations",
            "Vector rasterization, density/font scaling and device-pixel/screenshot equivalence",
            "WebView/browser navigation, session/network outcomes and runtime exception displayString()",
        ],
        "sources": sources.inventory(),
    }
    return {
        "baseline.json": (
            json.dumps(baseline, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
        ).encode("utf-8"),
        "assets/ic_launcher_foreground.xml": logo_bytes,
        "assets/LICENSE": license_bytes,
    }


def export_baseline(repo, output, check=False):
    output = Path(output).resolve()
    artifacts = build_export(repo)
    if check:
        errors = []
        for name, expected in artifacts.items():
            path = output / name
            if not path.is_file():
                errors.append("Missing baseline artifact: " + name)
            elif path.read_bytes() != expected:
                errors.append("Baseline drift: " + name)
        if errors:
            raise BaselineError("\n".join(errors))
    else:
        for name, data in artifacts.items():
            path = output / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    return artifacts


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo",
        required=True,
        type=Path,
        help="Canonical EhViewer source root (read only)",
    )
    parser.add_argument(
        "--output", required=True, type=Path, help="Baseline artifact directory"
    )
    parser.add_argument(
        "--check",
        "--verify",
        dest="check",
        action="store_true",
        help="Compare all generated bytes; write nothing; nonzero on missing/stale/unsupported sources",
    )
    args = parser.parse_args(argv)
    try:
        artifacts = export_baseline(args.repo, args.output, args.check)
        observed_head = read_head(args.repo.resolve())
    except (BaselineError, OSError, ET.ParseError, UnicodeError) as error:
        print(str(error), file=sys.stderr)
        return 1
    print(
        ("Verified" if args.check else "Exported")
        + f" {len(artifacts)} SignIn baseline artifacts"
    )
    print(
        f"Observed repository HEAD (not fixture data): {observed_head or 'unavailable'}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
