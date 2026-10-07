#!/usr/bin/env python3
"""Build an isolated debug Rust UI APK with GameActivity and AccessKit.

Only a platform loading bridge and Android framework dependencies run on the JVM.
This never installs an APK, opens production data, uses production signing keys,
or builds the legacy Kotlin application. Gradle owns framework DEX/resources;
Cargo owns the sole native application library and its patched activity glue.
"""

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT / "native"
MIN_SDK = 23
TARGET_SDK = 37
NDK_VERSION = "29.0.14206865"
BUILD_TOOLS_VERSION = "37.0.0"
APPLICATION_ID = "moe.tarsin.ehviewer.rust.prototype"
GAME_ACTIVITY_JNI = "Java_com_google_androidgamesdk_GameActivity_initializeNativeCode"
# Exact compiler support templates observed with pinned AGP 9.2.1 / Java 17.
# SDK stubs/synthetic companions are separately bound to their input owners.
D8_SUPPORT_CLASSES = {
    "Lcom/android/tools/r8/DesugarMethodHandlesLookup;",
    "Lcom/android/tools/r8/DesugarVarHandle;",
    "Lcom/android/tools/r8/DesugarVarHandle$0;",
    "Lcom/android/tools/r8/RecordTag;",
    "Lcom/android/tools/r8/annotations/LambdaMethod;",
}
ABIS = {
    "arm64-v8a": ("aarch64-linux-android", "aarch64-linux-android"),
    "x86_64": ("x86_64-linux-android", "x86_64-linux-android"),
    "armeabi-v7a": ("armv7-linux-androideabi", "armv7a-linux-androideabi"),
}


def validate_abis(abis: list[str]) -> None:
    """Reject empty, duplicate and unsupported ABI requests before any effects."""
    if not abis or len(set(abis)) != len(abis) or any(abi not in ABIS for abi in abis):
        raise ValueError("Expected distinct, supported requested ABIs")


def sdk_location(explicit: Path | None) -> Path:
    """Resolve an explicit SDK, standard SDK environment, or local.properties."""
    if explicit is not None:
        return explicit.expanduser().resolve()
    for name in ("ANDROID_HOME", "ANDROID_SDK_ROOT"):
        if value := os.environ.get(name):
            return Path(value).expanduser().resolve()
    properties = ROOT / "local.properties"
    if properties.is_file():
        for line in properties.read_text(encoding="utf-8").splitlines():
            key, separator, value = line.partition("=")
            if separator and key.strip() == "sdk.dir" and value.strip():
                return Path(value.strip()).expanduser().resolve()
    raise ValueError("Android SDK not found; pass --sdk or set ANDROID_HOME")


def native_environment(ndk: Path, abi: str) -> dict[str, str]:
    """Use target-specific Clang without changing host build-script compilers."""
    target, compiler = ABIS[abi]
    tools = ndk / "toolchains/llvm/prebuilt/linux-x86_64/bin"
    clang = tools / f"{compiler}{MIN_SDK}-clang"
    clangxx = tools / f"{compiler}{MIN_SDK}-clang++"
    ar = tools / "llvm-ar"
    for tool in (clang, clangxx, ar):
        if not tool.is_file():
            raise ValueError(f"Missing NDK tool: {tool}")
    environment = os.environ.copy()
    target_key = target.replace("-", "_")
    environment[f"CARGO_TARGET_{target_key.upper()}_LINKER"] = str(clang)
    environment[f"CC_{target_key}"] = str(clang)
    environment[f"CXX_{target_key}"] = str(clangxx)
    environment[f"AR_{target_key}"] = str(ar)
    environment["ANDROID_NDK_HOME"] = str(ndk)
    environment["ANDROID_NDK_ROOT"] = str(ndk)
    environment["CARGO_TARGET_DIR"] = str(NATIVE / "target")
    return environment


def verify_android_features(metadata: dict) -> None:
    """Reject a buildable no-op accessibility backend and conflicting Activity features."""
    packages = {package["id"]: package["name"] for package in metadata["packages"]}
    nodes = metadata.get("resolve", {}).get("nodes", [])
    required = {
        "eframe": {"accesskit", "android-game-activity"},
        "winit": {"android-game-activity"},
        "accesskit_winit": {"accesskit_android"},
        "accesskit_android": {"embedded-dex"},
        "android-activity": {"game-activity"},
    }
    for name, features in required.items():
        matches = [
            set(node["features"]) for node in nodes if packages.get(node["id"]) == name
        ]
        if len(matches) != 1 or not features <= matches[0]:
            raise ValueError(
                f"Android feature contract missing for {name}: {sorted(features)}"
            )
        if matches[0] & {"android-native-activity", "native-activity"}:
            raise ValueError("Retired NativeActivity feature is enabled")


def verify_exports(symbols: set[str]) -> None:
    """Require the actual GameActivity bridge, not the retired NativeActivity entry."""
    if not {"GameActivity_onCreate", "android_main", GAME_ACTIVITY_JNI} <= symbols:
        raise ValueError("GameActivity entry symbols are missing")
    if "ANativeActivity_onCreate" in symbols:
        raise ValueError("Retired NativeActivity entry is present")
    if any(symbol.startswith("Java_com_hippo_ehviewer_") for symbol in symbols):
        raise ValueError("Bootstrap unexpectedly links the legacy business JNI facade")


def verify_payload(apk: Path, abis: list[str]) -> None:
    """Check exact ELF ABIs and regular framework DEX; SDK dexdump validates classes."""
    validate_abis(abis)
    expected = {f"lib/{abi}/libeh_native.so" for abi in abis}
    with zipfile.ZipFile(apk) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("APK contains duplicate ZIP members")
        dex_files = [name for name in names if name.endswith(".dex")]
        if "classes.dex" not in dex_files or any(
            not re.fullmatch(r"classes(?:[2-9]|[1-9][0-9]+)?\.dex", name)
            for name in dex_files
        ):
            raise ValueError("GameActivity requires regular framework DEX payloads")
        if not 1 <= len(dex_files) <= 128:
            raise ValueError("APK DEX count exceeds the inspection budget")
        for name in dex_files:
            if archive.getinfo(name).file_size > 32 * 1024 * 1024:
                raise ValueError("APK DEX exceeds the inspection budget")
            with archive.open(name) as dex:
                if not re.fullmatch(rb"dex\n0(?:35|37|38|39|40|41)\x00", dex.read(8)):
                    raise ValueError("APK DEX header is invalid")
        libraries = {name for name in names if name.endswith(".so")}
        if libraries != expected:
            raise ValueError("APK native payload does not match requested ABIs")
        elf_abis = {"arm64-v8a": (2, 183), "x86_64": (2, 62), "armeabi-v7a": (1, 40)}
        for abi in abis:
            with archive.open(f"lib/{abi}/libeh_native.so") as library:
                header = library.read(64)
            if len(header) < 20 or header[:4] != b"\x7fELF":
                raise ValueError("APK contains a non-ELF native payload")
            elf_class, machine = elf_abis[abi]
            if (
                len(header) < (64 if elf_class == 2 else 52)
                or header[4:7] != bytes([elf_class, 1, 1])
                or int.from_bytes(header[16:18], "little") != 3
                or int.from_bytes(header[18:20], "little") != machine
            ):
                raise ValueError("ELF header does not match the requested ABI")


def framework_class_descriptors(
    origins: Path, platform: Path | None = None
) -> set[str]:
    """Bind allowed classes to checksummed external Gradle artifacts, including generated R."""
    result = set()
    for row in origins.read_text(encoding="utf-8").splitlines():
        fields = row.split("\t")
        if len(fields) != 3:
            raise ValueError("Invalid framework artifact origin record")
        coordinate, filename, expected_hash = fields
        group = coordinate.split(":")[0]
        if not (
            group.startswith("androidx.")
            or group
            in {
                "org.jetbrains",
                "org.jetbrains.kotlin",
                "org.jetbrains.kotlinx",
                "org.jspecify",
                "com.google.guava",
            }
        ):
            raise ValueError("Unreviewed framework artifact coordinate")
        path = Path(filename)
        if (
            path.suffix not in {".aar", ".jar"}
            or path.stat().st_size > 32 * 1024 * 1024
        ):
            raise ValueError("Unsupported or oversized framework artifact")
        payload = path.read_bytes()
        if hashlib.sha256(payload).hexdigest() != expected_hash:
            raise ValueError("Framework artifact changed after Gradle resolution")
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            jars = (
                [payload]
                if path.suffix == ".jar"
                else [
                    archive.read(name)
                    for name in archive.namelist()
                    if name == "classes.jar"
                    or (name.startswith("libs/") and name.endswith(".jar"))
                ]
            )
            if path.suffix == ".aar" and "AndroidManifest.xml" in archive.namelist():
                manifest = archive.read("AndroidManifest.xml")
                if (
                    len(manifest) > 2 * 1024 * 1024
                    or b"<!DOCTYPE" in manifest
                    or b"<!ENTITY" in manifest
                ):
                    raise ValueError("Unsupported framework resource manifest")
                # Checksummed Gradle framework XML only; DTD/entities and size rejected above.
                namespace = ET.fromstring(manifest).get("package")  # noqa: S314
                if namespace:
                    stem = "L" + namespace.replace(".", "/") + "/R"
                    result.add(stem + ";")
                    if "R.txt" in archive.namelist():
                        for declaration in (
                            archive.read("R.txt").decode("utf-8").splitlines()
                        ):
                            parts = declaration.split()
                            if len(parts) >= 2 and re.fullmatch(r"[a-z_]+", parts[1]):
                                result.add(stem + "$" + parts[1] + ";")
        for jar in jars:
            with zipfile.ZipFile(io.BytesIO(jar)) as archive:
                result.update(
                    "L" + name.removesuffix(".class") + ";"
                    for name in archive.namelist()
                    if name.endswith(".class") and not name.startswith("META-INF/")
                )
    if platform is not None:
        # D8 may synthesize stubs for newer Android API classes to run on minSdk 23.
        # Bind these names to the actual compile-SDK boot classpath, not an android/* prefix.
        with zipfile.ZipFile(platform) as archive:
            result.update(
                "L" + name.removesuffix(".class") + ";"
                for name in archive.namelist()
                if name.endswith(".class") and not name.startswith("META-INF/")
            )
    if not result:
        raise ValueError("Framework class origin inventory is empty")
    return result


def verify_class_descriptors(descriptors: set[str], framework: set[str]) -> None:
    """Permit only generated resources and the thin loader in our app namespace."""
    namespace = "Lmoe/tarsin/ehviewer/rust/prototype/"
    required = {
        namespace + "BootstrapActivity;",
        "Lcom/google/androidgamesdk/GameActivity;",
        "Lcom/google/androidgamesdk/GameActivity$InputEnabledSurfaceView;",
    }
    if not required <= descriptors:
        raise ValueError("APK is missing GameActivity or its loading bridge")
    for descriptor in descriptors:
        if descriptor.startswith(("Lcom/hippo/ehviewer/", "Lcom/ehviewer/core/")):
            raise ValueError("APK contains a legacy application class")
        if descriptor.startswith(namespace) and not (
            descriptor == namespace + "BootstrapActivity;"
            or descriptor == namespace + "R;"
            or descriptor == namespace + "R$style;"
        ):
            raise ValueError(
                "APK has application logic outside the Rust/loading boundary"
            )
        if descriptor.startswith(namespace):
            continue
        # Exact pinned-compiler templates, not an entire package-prefix grant.
        if descriptor in D8_SUPPORT_CLASSES:
            continue
        # D8 synthetic classes require an owning checksummed input class.
        synthetic_owner = descriptor.split("$$ExternalSynthetic", 1)[0] + ";"
        companion_owner = descriptor.removesuffix("$-CC;") + ";"
        if (
            descriptor not in framework
            and not (
                "$$ExternalSynthetic" in descriptor and synthetic_owner in framework
            )
            and not (descriptor.endswith("$-CC;") and companion_owner in framework)
        ):
            raise ValueError(
                f"APK has a JVM class with no framework artifact origin: {descriptor[:240]!r}"
            )


def verify_android_classes(apk: Path, tools: Path, framework: set[str]) -> None:
    """Inspect real class definitions with SDK dexdump, not raw string matching."""
    descriptors = set()
    with (
        tempfile.TemporaryDirectory(prefix="eh-rust-dex-") as temporary,
        zipfile.ZipFile(apk) as archive,
    ):
        for name in archive.namelist():
            if not name.endswith(".dex"):
                continue
            if not re.fullmatch(r"classes(?:[2-9]|[1-9][0-9]+)?\.dex", name):
                raise ValueError("Unexpected DEX member path")
            if archive.getinfo(name).file_size > 32 * 1024 * 1024:
                raise ValueError("APK DEX exceeds the inspection budget")
            destination = Path(temporary) / Path(name).name
            destination.write_bytes(archive.read(name))
            # DEX strings use modified UTF-8 (including surrogate encodings). Keep
            # dump bytes intact; decode only class descriptors, never unrelated literals.
            output = subprocess.check_output([str(tools / "dexdump"), str(destination)])
            descriptors.update(
                descriptor.decode("utf-8", errors="surrogateescape")
                for descriptor in re.findall(
                    rb"Class descriptor\s*:\s*'([^']+)'", output
                )
            )
    verify_class_descriptors(descriptors, framework)


def verify_apk_metadata(metadata: str) -> None:
    """Accept only prototype identity, SDK/entry contract and framework receiver permission."""
    package = re.findall(r"^package: name='([^']+)'", metadata, re.MULTILINE)
    minimum = re.findall(
        r"^(?:minSdkVersion|sdkVersion):'(\d+)'$", metadata, re.MULTILINE
    )
    target = re.findall(r"^targetSdkVersion:'(\d+)'$", metadata, re.MULTILINE)
    launcher = re.findall(
        r"^launchable-activity: name='([^']+)'", metadata, re.MULTILINE
    )
    if (
        package != [APPLICATION_ID]
        or minimum != [str(MIN_SDK)]
        or target != [str(TARGET_SDK)]
        or launcher != [APPLICATION_ID + ".BootstrapActivity"]
    ):
        raise ValueError("APK identity, SDK level or Activity contract drifted")
    permissions = set(
        re.findall(
            r"^uses-permission(?:-sdk-\d+)?: name='([^']+)'", metadata, re.MULTILINE
        )
    )
    if permissions - {APPLICATION_ID + ".DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"}:
        raise ValueError("APK has permissions outside the offline bootstrap boundary")


def run(*args: str | Path, environment: dict[str, str] | None = None) -> None:
    """Execute without a shell; paths containing spaces stay single arguments."""
    subprocess.run([str(arg) for arg in args], cwd=ROOT, env=environment, check=True)


def stage_libraries(sdk: Path, abis: list[str], jobs: int) -> Path:
    """Build and validate before replacing disposable staged JNI libraries."""
    validate_abis(abis)
    if sys.platform != "linux" or jobs < 1:
        raise ValueError("Linux host and positive --jobs are required")
    ndk = sdk / "ndk" / NDK_VERSION
    environments = {abi: native_environment(ndk, abi) for abi in abis}
    tools = ndk / "toolchains/llvm/prebuilt/linux-x86_64/bin"
    output = NATIVE / "build/game-activity"
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="jni-", dir=output) as temporary:
        candidate = Path(temporary)
        for abi, environment in environments.items():
            target, _ = ABIS[abi]
            metadata = subprocess.check_output(
                [
                    "cargo",
                    "metadata",
                    "--locked",
                    "--format-version",
                    "1",
                    "--manifest-path",
                    str(NATIVE / "Cargo.toml"),
                    "--filter-platform",
                    target,
                ],
                env=environment,
                text=True,
            )
            try:
                verify_android_features(json.loads(metadata))
            except (ValueError, KeyError, TypeError, AttributeError) as error:
                raise ValueError(
                    f"Android dependency feature verification failed: {error}"
                ) from error
            run(
                "cargo",
                "build",
                "--locked",
                "--manifest-path",
                NATIVE / "Cargo.toml",
                "--lib",
                "--target",
                target,
                "--jobs",
                str(jobs),
                environment=environment,
            )
            library = NATIVE / "target" / target / "debug/libeh_native.so"
            staged = candidate / abi / "libeh_native.so"
            staged.parent.mkdir(parents=True)
            run(tools / "llvm-strip", "--strip-unneeded", "-o", staged, library)
            for path in (library, staged):
                exports = subprocess.check_output(
                    [str(tools / "llvm-nm"), "-D", "--defined-only", str(path)],
                    text=True,
                )
                verify_exports(
                    {line.split()[-1] for line in exports.splitlines() if line.split()}
                )
        destination = output / "jni"
        if destination.exists():
            # Generated staging only; never touch user files, Cargo artifacts or production data.
            try:
                shutil.rmtree(destination)
            except OSError as error:
                raise ValueError(
                    "Cannot replace generated JNI staging directory"
                ) from error
        shutil.copytree(candidate, destination)
    return destination


def build(sdk: Path, abis: list[str], jobs: int) -> Path:
    """Gradle builds only the new isolated application; verify before publishing APK."""
    validate_abis(abis)
    if sys.platform != "linux" or jobs < 1:
        raise ValueError("Linux host and positive --jobs are required")
    tools = sdk / "build-tools" / BUILD_TOOLS_VERSION
    for tool in ("aapt2", "zipalign", "apksigner", "dexdump"):
        if not (tools / tool).is_file():
            raise ValueError(f"Missing Android build tool: {tools / tool}")
    environment = os.environ.copy()
    environment["ANDROID_HOME"] = str(sdk)
    environment["ANDROID_SDK_ROOT"] = str(sdk)
    run(
        ROOT / "gradlew",
        "--no-daemon",
        "--max-workers=2",
        "-p",
        NATIVE / "android",
        "assembleDebug",
        f"-PnativeAbis={','.join(abis)}",
        f"-PnativeJobs={jobs}",
        environment=environment,
    )
    candidate = (
        NATIVE / "android/build/outputs/apk/debug/EhViewerRustBootstrap-debug.apk"
    )
    verify_payload(candidate, abis)
    platform = next(
        (
            sdk / "platforms" / name / "android.jar"
            for name in (f"android-{TARGET_SDK}", f"android-{TARGET_SDK}.0")
            if (sdk / "platforms" / name / "android.jar").is_file()
        ),
        None,
    )
    if platform is None:
        raise ValueError("Compile SDK boot classpath is missing")
    framework = framework_class_descriptors(
        NATIVE / "android/build/framework-artifacts.tsv", platform
    )
    verify_android_classes(candidate, tools, framework)
    run(tools / "zipalign", "-c", "-P", "16", "4", candidate)
    run(tools / "apksigner", "verify", "--verbose", candidate)
    metadata = subprocess.check_output(
        [str(tools / "aapt2"), "dump", "badging", str(candidate)], text=True
    )
    verify_apk_metadata(metadata)
    destination = NATIVE / "build/android/ehviewer-rust-bootstrap.apk"
    destination.parent.mkdir(parents=True, exist_ok=True)
    unpublished = destination.with_suffix(".candidate.apk")
    shutil.copyfile(candidate, unpublished)
    unpublished.replace(destination)
    return destination


def main() -> None:
    """Building never installs or launches an application."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path)
    parser.add_argument("--abi", action="append", choices=tuple(ABIS))
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument(
        "--compile-only",
        action="store_true",
        help="Gradle hook: stage native libraries only",
    )
    args = parser.parse_args()
    abis = list(dict.fromkeys(args.abi or ["arm64-v8a"]))
    try:
        sdk = sdk_location(args.sdk)
        artifact = (
            stage_libraries(sdk, abis, args.jobs)
            if args.compile_only
            else build(sdk, abis, args.jobs)
        )
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Native bootstrap build failed: {error}\n")
    print(f"Built debug bootstrap component (not installed): {artifact}")


if __name__ == "__main__":
    main()
