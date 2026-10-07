"""Offline contracts for the isolated Rust GameActivity packager."""

import hashlib
import io
import os
import tempfile
import unittest
import warnings
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path
from unittest.mock import patch

import tomllib

from scripts import build_native

ANDROID = "{http://schemas.android.com/apk/res/android}"
LOADER = "Lmoe/tarsin/ehviewer/rust/prototype/BootstrapActivity;"
GAME_ACTIVITY = "Lcom/google/androidgamesdk/GameActivity;"
SURFACE = "Lcom/google/androidgamesdk/GameActivity$InputEnabledSurfaceView;"
FRAMEWORK = {GAME_ACTIVITY, SURFACE, "Landroidx/appcompat/app/AppCompatActivity;"}


class NativePackagingTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def archive(self, names, include_dex=True):
        path = self.root / "test.apk"
        names = list(names)
        if include_dex and "classes.dex" not in names:
            names.append("classes.dex")
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(path, "w") as archive:
                for name in names:
                    if name.endswith(".dex"):
                        # Shape fixture only; real DEX definitions use SDK dexdump below.
                        archive.writestr(name, b"dex\n035\x00" + bytes(104))
                        continue
                    header = bytearray(64)
                    header[:4] = b"\x7fELF"
                    elf_class, machine = (
                        (1, 40)
                        if "/armeabi-v7a/" in name
                        else (2, 62)
                        if "/x86_64/" in name
                        else (2, 183)
                    )
                    header[4:7] = bytes([elf_class, 1, 1])
                    header[16:18] = (3).to_bytes(2, "little")
                    header[18:20] = machine.to_bytes(2, "little")
                    archive.writestr(name, header)
        return path

    def test_exact_requested_abis_are_required(self):
        path = self.archive(
            [
                "AndroidManifest.xml",
                "lib/arm64-v8a/libeh_native.so",
                "lib/x86_64/libeh_native.so",
            ]
        )
        build_native.verify_payload(path, ["arm64-v8a", "x86_64"])
        for abis in (["arm64-v8a"], ["armeabi-v7a"]):
            with (
                self.subTest(abis=abis),
                self.assertRaisesRegex(ValueError, "requested ABIs"),
            ):
                build_native.verify_payload(path, abis)

    def test_framework_dex_required_but_hidden_dex_and_duplicate_members_rejected(self):
        library = "lib/arm64-v8a/libeh_native.so"
        for members, include_dex, error in [
            ([library], False, "regular framework DEX"),
            ([library, "assets/hidden.dex"], True, "regular framework DEX"),
            ([library, "../escape.dex"], True, "regular framework DEX"),
            ([library, library], True, "duplicate ZIP"),
        ]:
            with (
                self.subTest(members=members),
                self.assertRaisesRegex(ValueError, error),
            ):
                build_native.verify_payload(
                    self.archive(members, include_dex), ["arm64-v8a"]
                )

    def test_invalid_dex_header_and_non_elf_library_are_rejected(self):
        for dex, elf, error in [
            (b"not-dex", b"\x7fELF" + bytes(60), "DEX header"),
            (b"dex\n035\x00" + bytes(104), b"not-native", "non-ELF"),
        ]:
            path = self.root / "bad.apk"
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr("classes.dex", dex)
                archive.writestr("lib/arm64-v8a/libeh_native.so", elf)
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                build_native.verify_payload(path, ["arm64-v8a"])

    def test_invalid_abi_lists_and_mislabeled_elf_are_rejected(self):
        path = self.archive(["lib/arm64-v8a/libeh_native.so"])
        for abis in ([], ["unknown"], ["arm64-v8a", "arm64-v8a"]):
            with (
                self.subTest(abis=abis),
                self.assertRaisesRegex(ValueError, "requested ABIs"),
            ):
                build_native.verify_payload(path, abis)
        with zipfile.ZipFile(path) as archive:
            header = archive.read("lib/arm64-v8a/libeh_native.so")
            dex = archive.read("classes.dex")
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("lib/x86_64/libeh_native.so", header)
            archive.writestr("classes.dex", dex)
        with self.assertRaisesRegex(ValueError, "ELF header"):
            build_native.verify_payload(path, ["x86_64"])

    def test_multidex_double_digit_names_are_valid_but_number_one_and_zero_are_not(
        self,
    ):
        library = "lib/arm64-v8a/libeh_native.so"
        valid = self.archive([library, "classes2.dex", "classes10.dex"])
        build_native.verify_payload(valid, ["arm64-v8a"])
        for name in (
            "classes0.dex",
            "classes1.dex",
            "classes01.dex",
            "classes2.dex/hidden.dex",
        ):
            with (
                self.subTest(name=name),
                self.assertRaisesRegex(ValueError, "regular framework DEX"),
            ):
                build_native.verify_payload(
                    self.archive([library, name]), ["arm64-v8a"]
                )

    def test_debug_metadata_accepts_sdk_tool_spelling_and_rejects_identity_permission_drift(
        self,
    ):
        metadata = (
            f"package: name='{build_native.APPLICATION_ID}' versionCode='2'\n"
            "minSdkVersion:'23'\n"
            "targetSdkVersion:'37'\n"
            f"launchable-activity: name='{build_native.APPLICATION_ID}.BootstrapActivity' label=''\n"
            f"uses-permission: name='{build_native.APPLICATION_ID}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION'\n"
        )
        build_native.verify_apk_metadata(metadata)
        build_native.verify_apk_metadata(
            metadata.replace("minSdkVersion:", "sdkVersion:")
        )
        for changed in (
            metadata.replace(build_native.APPLICATION_ID, "wrong.application"),
            metadata.replace("minSdkVersion:'23'", "minSdkVersion:'26'"),
            metadata.replace("targetSdkVersion:'37'", "targetSdkVersion:'34'"),
            metadata.replace("BootstrapActivity", "NativeActivity"),
            metadata + "sdkVersion:'23'\n",
        ):
            with (
                self.subTest(metadata=changed),
                self.assertRaisesRegex(ValueError, "contract drifted"),
            ):
                build_native.verify_apk_metadata(changed)
        for permission in (
            "android.permission.INTERNET",
            "android.permission.READ_EXTERNAL_STORAGE",
            "android.permission.REQUEST_INSTALL_PACKAGES",
        ):
            with (
                self.subTest(permission=permission),
                self.assertRaisesRegex(ValueError, "offline bootstrap boundary"),
            ):
                build_native.verify_apk_metadata(
                    metadata + f"uses-permission: name='{permission}'\n"
                )

    def test_game_activity_exports_required_and_old_entries_rejected(self):
        symbols = {
            "android_main",
            "GameActivity_onCreate",
            build_native.GAME_ACTIVITY_JNI,
        }
        build_native.verify_exports(symbols)
        for change, error in [
            (symbols - {"GameActivity_onCreate"}, "GameActivity entry"),
            (symbols | {"ANativeActivity_onCreate"}, "Retired NativeActivity"),
            (
                symbols | {"Java_com_hippo_ehviewer_bridge_nativeExample"},
                "legacy business JNI",
            ),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                build_native.verify_exports(change)

    def test_only_platform_loader_and_generated_resources_allowed_in_app_dex(self):
        descriptors = {LOADER, GAME_ACTIVITY, SURFACE}
        build_native.verify_class_descriptors(
            descriptors | {"Landroidx/appcompat/app/AppCompatActivity;"}, FRAMEWORK
        )
        build_native.verify_class_descriptors(
            descriptors | {"Lmoe/tarsin/ehviewer/rust/prototype/R$style;"}, FRAMEWORK
        )
        for changed, error in [
            (descriptors - {SURFACE}, "missing GameActivity"),
            (
                descriptors | {"Lcom/hippo/ehviewer/ui/MainActivity;"},
                "legacy application",
            ),
            (
                descriptors | {"Lcom/ehviewer/core/database/EhDatabase;"},
                "legacy application",
            ),
            (
                descriptors | {"Lmoe/tarsin/ehviewer/rust/prototype/LoginLogic;"},
                "application logic",
            ),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                build_native.verify_class_descriptors(changed, FRAMEWORK)

    def test_unknown_namespaces_and_unowned_synthetics_are_rejected(self):
        required = {LOADER, GAME_ACTIVITY, SURFACE}
        synthetic = GAME_ACTIVITY.removesuffix(";") + "$$ExternalSyntheticLambda0;"
        build_native.verify_class_descriptors(required | {synthetic}, FRAMEWORK)
        companion = GAME_ACTIVITY.removesuffix(";") + "$-CC;"
        build_native.verify_class_descriptors(required | {companion}, FRAMEWORK)
        build_native.verify_class_descriptors(
            required | build_native.D8_SUPPORT_CLASSES, FRAMEWORK
        )
        for unknown in (
            "Lcom/acme/LoginLogic;",
            "Lcom/acme/LoginLogic$$ExternalSyntheticLambda0;",
            "Landroidx/appcompat/app/UnexpectedLoginLogic;",
            "Lcom/android/tools/r8/UnexpectedLoginLogic;",
        ):
            with (
                self.subTest(unknown=unknown),
                self.assertRaisesRegex(ValueError, "no framework artifact origin"),
            ):
                build_native.verify_class_descriptors(required | {unknown}, FRAMEWORK)

    def test_framework_inventory_binds_checksums_classes_and_generated_resources(self):
        jar = io.BytesIO()
        with zipfile.ZipFile(jar, "w") as archive:
            archive.writestr(
                "com/google/androidgamesdk/GameActivity.class", b"fixture-only"
            )
        artifact = self.root / "framework.aar"
        with zipfile.ZipFile(artifact, "w") as archive:
            archive.writestr("classes.jar", jar.getvalue())
            archive.writestr(
                "AndroidManifest.xml", '<manifest package="androidx.fixture"/>'
            )
            archive.writestr("R.txt", "int style Test 0x7f000001\n")
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        origins = self.root / "origins.tsv"
        origins.write_text(f"androidx.fixture:framework:1.0\t{artifact}\t{digest}\n")
        self.assertEqual(
            build_native.framework_class_descriptors(origins),
            {
                GAME_ACTIVITY,
                "Landroidx/fixture/R;",
                "Landroidx/fixture/R$style;",
            },
        )
        origins.write_text(f"androidx.fixture:framework:1.0\t{artifact}\t{'0' * 64}\n")
        with self.assertRaisesRegex(ValueError, "changed after Gradle"):
            build_native.framework_class_descriptors(origins)
        origins.write_text(f"com.acme:application:1.0\t{artifact}\t{digest}\n")
        with self.assertRaisesRegex(ValueError, "Unreviewed framework"):
            build_native.framework_class_descriptors(origins)

    def test_android_features_reject_noop_adapter_and_native_activity(self):
        required = {
            "eframe": ["accesskit", "android-game-activity"],
            "winit": ["android-game-activity"],
            "accesskit_winit": ["accesskit_android"],
            "accesskit_android": ["embedded-dex"],
            "android-activity": ["game-activity"],
        }
        metadata = {
            "packages": [{"id": name, "name": name} for name in required],
            "resolve": {
                "nodes": [
                    {"id": name, "features": features}
                    for name, features in required.items()
                ]
            },
        }
        build_native.verify_android_features(metadata)
        metadata["resolve"]["nodes"][2]["features"] = []
        with self.assertRaisesRegex(ValueError, "accesskit_winit"):
            build_native.verify_android_features(metadata)
        metadata["resolve"]["nodes"][2]["features"] = ["accesskit_android"]
        metadata["resolve"]["nodes"][0]["features"].append("android-native-activity")
        with self.assertRaisesRegex(ValueError, "Retired NativeActivity"):
            build_native.verify_android_features(metadata)

    def test_android_adapter_and_nonincremental_cargo_hook_remain_explicit(self):
        cargo = tomllib.loads(
            (build_native.NATIVE / "Cargo.toml").read_text(encoding="utf-8")
        )
        android = cargo["target"]['cfg(target_os = "android")']["dependencies"]
        self.assertIn("accesskit_android", android["accesskit_winit"]["features"])
        self.assertIn("android-game-activity", android["eframe"]["features"])
        self.assertIn("accesskit", cargo["dependencies"]["eframe"]["features"])
        gradle = (build_native.NATIVE / "android/build.gradle.kts").read_text(
            encoding="utf-8"
        )
        self.assertIn('repositoryRoot.resolve("app/src/main/rust/Cargo.toml")', gradle)
        self.assertIn("outputs.upToDateWhen { false }", gradle)

    def test_sdk_dexdump_reads_actual_definitions_and_blocks_member_traversal(self):
        path = self.archive(["lib/arm64-v8a/libeh_native.so"])
        output = "\n".join(
            f"  Class descriptor  : '{descriptor}'"
            for descriptor in (LOADER, GAME_ACTIVITY, SURFACE)
        )
        # Real dexdump includes modified UTF-8 literals unrelated to class names.
        raw_output = (
            output.encode("utf-8") + b"\nString literal: " + bytes.fromhex("ed a0 bd")
        )
        with patch("subprocess.check_output", return_value=raw_output) as dump:
            build_native.verify_android_classes(path, self.root, FRAMEWORK)
            self.assertEqual(dump.call_args.args[0][0], str(self.root / "dexdump"))
        malicious = self.archive(["../escape.dex"])
        with (
            self.assertRaisesRegex(ValueError, "Unexpected DEX"),
            patch("subprocess.check_output") as dump,
        ):
            build_native.verify_android_classes(malicious, self.root, FRAMEWORK)
        dump.assert_not_called()

    def test_ndk_environment_preserves_host_compiler_and_supports_armv7(self):
        tools = self.root / "toolchains/llvm/prebuilt/linux-x86_64/bin"
        tools.mkdir(parents=True)
        for name in [
            "armv7a-linux-androideabi23-clang",
            "armv7a-linux-androideabi23-clang++",
            "llvm-ar",
        ]:
            (tools / name).touch()
        with patch.dict(os.environ, {"CC": "host-compiler"}, clear=True):
            environment = build_native.native_environment(self.root, "armeabi-v7a")
        self.assertEqual(environment["CC"], "host-compiler")
        self.assertEqual(
            environment["CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER"],
            str(tools / "armv7a-linux-androideabi23-clang"),
        )
        self.assertNotIn("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER", environment)
        with self.assertRaisesRegex(ValueError, "Missing NDK tool"):
            build_native.native_environment(self.root, "arm64-v8a")

    def test_sdk_precedence_and_missing_sdk_are_explicit(self):
        with patch.dict(
            os.environ, {"ANDROID_HOME": str(self.root / "env")}, clear=True
        ):
            self.assertEqual(build_native.sdk_location(self.root), self.root)
            self.assertEqual(build_native.sdk_location(None), self.root / "env")
        with (
            patch.dict(os.environ, {}, clear=True),
            patch.object(build_native, "ROOT", self.root),
        ):
            with self.assertRaisesRegex(ValueError, "Android SDK not found"):
                build_native.sdk_location(None)
            (self.root / "local.properties").write_text(
                f"sdk.dir={self.root / 'local sdk'}\n", encoding="utf-8"
            )
            self.assertEqual(build_native.sdk_location(None), self.root / "local sdk")

    def test_manifest_is_isolated_has_no_permissions_and_retains_api_23(self):
        # Trusted tracked fixture, never user-supplied XML.
        manifest = ET.parse(build_native.NATIVE / "AndroidManifest.xml").getroot()  # noqa: S314
        self.assertEqual(manifest.findall("uses-sdk"), [])
        # AGP 9+ SDK contracts live in Gradle, then are checked in compiled APK metadata.
        gradle = (build_native.NATIVE / "android/build.gradle.kts").read_text(
            encoding="utf-8"
        )
        self.assertIn(f"minSdk = {build_native.MIN_SDK}", gradle)
        self.assertIn(f"targetSdk = {build_native.TARGET_SDK}", gradle)
        self.assertIn(f'applicationId = "{build_native.APPLICATION_ID}"', gradle)
        self.assertEqual(manifest.findall("uses-permission"), [])
        [application] = manifest.findall("application")
        self.assertNotEqual(application.get(ANDROID + "hasCode"), "false")
        self.assertEqual(application.get(ANDROID + "allowBackup"), "false")
        [activity] = application.findall("activity")
        self.assertEqual(activity.get(ANDROID + "name"), ".BootstrapActivity")
        self.assertEqual(activity.get(ANDROID + "launchMode"), "singleTask")
        [metadata] = activity.findall("meta-data")
        self.assertEqual(metadata.get(ANDROID + "value"), "eh_native")


if __name__ == "__main__":
    unittest.main()
