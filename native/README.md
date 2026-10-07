# Rust application bootstrap

**Not a feature-complete EhViewer replacement.** This is an isolated foundation
for the [full Rust rewrite](../docs/rewrite/full-rust.md). Product UI must match
the [original presentation and interaction contracts](../docs/rewrite/ui-parity.md).
Stock egui controls are not accepted as equivalent Material3 widgets.

## Current implementation

- Rust egui/eframe diagnostic UI and direct in-process `eh_net` URL parsing.
- Android GameActivity/Glow entry; one project-owned Java class only loads the
  native library and inherits platform lifecycle/input integration.
- Explicit `accesskit_winit/accesskit_android` plus embedded AccessKit framework
  DEX in the Rust dependency. Cargo's resolved target features are checked.
  Enabling eframe's generic AccessKit feature alone selects Android's no-op
  adapter; that configuration is rejected.
- Pure Rust original sign-in interaction reducer and 18 regressions. It owns
  no credentials, HTTP, cookies or session authority. **It is not yet connected
  to a product SignIn renderer/authentication runtime.**
- Source-bound original sign-in layout, vector/license, 12 locales, errors and
  theme references under `parity/`. Text is raw resource XML, not a claim that
  Android locale/escape resolution has been reproduced.
- Original-vector Rust renderer and source-drift/code-generation check. Diagnostic asset view only, not the product sign-in page or pixel equivalence.
- Pure Rust source-anchored logical-dp placement, 19 regressions, measured-widget boundary and one shared expanded scroll. Density rounding and real Material measurements remain unaccepted.
- Pinned Compose secure-text metadata policy, 12 Rust regressions and 17 complete class proofs with 24 drift/checker tests. Preserves 1500ms reveal, UTF-16/codepoint quirks, mode/platform/focus rules and redacted state. Not wired into product renderer or IME; clipboard, undo, autofill and timer-delivery acceptance remain open. See `parity/secure-text/README.md`.
- Rust DataStore Preferences1.2.1 valid-UTF8 format and synthetic host file runtime, 38 real JVM cases and 25 focused Rust regressions. Retains untouched wire/unknown fields, ordered sets and signed/float bits; failed precommit operations preserve old bytes, committed directory-sync failures remain explicit. No application storage call/path discovery. Android locking, malformed-value UTF8 parity, settings migration/UI and device durability remain pending. See `parity/preferences/README.md`.
- Bounded std-only pinned-JVM UTF8 replacement decoder, 258,862 real generated PreferenceMap value/set cases, 5 Rust regressions and 14 proof/runtime tests. One-MiB input/output bounds and redacted errors; original strict key/value codec remains unchanged. Host JDK proof is not Android libcore/device parity. See `parity/jvm-utf8/README.md`.
- Original core/UI + default/marshmallow debug runtime dependency graphs bound offline; selected versions/source hashes under `parity/fixtures/signin/`.
- Minimal vendored winit0.30.13 raw Android input hook; one existing queue owner, borrowed callback before normal dispatch. Original source/license retained, bounded patch/provenance under `vendor/winit/docs/`. Global Cargo registry is unchanged.
- Rust full-buffer/UTF-16 selection/composition transport and local tagged callback leases with ordered local action snapshots, redacted Debug and fail-closed bounded queue. No native-session provenance or zeroization guarantee.
- Android diagnostic IME page reaches SDK editor configuration, complete text/action route and repaint. Only dummy multiline text; not the original secure field or authentication. Local action snapshots cannot recover the SDK's overwritten actions or atomic text/action ordering.
- Android prototype `run()` is one-shot per process, including failed first invocation: re-entry rejects before eframe's cached event loop can bind a new app to its old handler. App drop releases retained GUI context/session and best-effort resets owned SDK buffer. Native reset ownership survives local errors; idle frames do not reset another widget's buffer. Suspended/OS-destroyed device lifecycle remains unaccepted.
- Separate prototype APK identity, no network/storage/install permissions.
  Framework receiver protection permission can be present after manifest merge.

No Compose, Room, Ktor or old application/JNI business facade is built into this
target. Third-party AndroidX, Kotlin runtime, GameActivity and AccessKit framework
internals remain dependencies, not hidden project-owned Kotlin business logic.

Not implemented: actual authentication, HTTP, application persistence, catalog/product UI,
reader, downloads, background services and complete localization/platform effects.
Diagnostic UI still uses English/default fonts. URL inspection never opens a gallery.

## Checks

Use repository-pinned Rust toolchain and SDK. Tests never open production data.

```sh
cargo fmt --manifest-path native/Cargo.toml --check
# Generate/check the pinned synthetic JVM fixture before aggregate Rust tests.
# Keep the same JVM_UTF8_BUILD environment for Cargo if overridden.
python3 -B native/parity/jvm-utf8/verify.py
cargo test --locked --manifest-path native/Cargo.toml --jobs 2
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets --jobs 2 -- -D warnings
python3 -B -m unittest scripts.tests.test_build_native scripts.tests.test_check_native_coverage scripts.tests.test_export_signin_artwork
python3 -B scripts/export_signin_artwork.py --check
python3 -B -m unittest discover -s native/parity/tests
python3 -B native/parity/tools/export_signin_baseline.py --repo . --output native/parity/fixtures/signin --check
cargo llvm-cov --locked --manifest-path native/Cargo.toml --package eh_native --lib --tests --json --output-path native/build/coverage/coverage.json --jobs 2
python3 -B scripts/check_native_coverage.py native/build/coverage/coverage.json
python3 -B scripts/check_native_coverage.py native/build/coverage/coverage.json --owner native/src/signin_layout.rs
python3 -B scripts/check_native_coverage.py native/build/coverage/coverage.json --owner native/src/signin_artwork.rs
python3 -B scripts/check_native_coverage.py native/build/coverage/coverage.json --owner native/src/ime_session.rs
python3 -B native/vendor/winit/tests/android_input_handler.py -v
bash native/parity/secure-text/check_host.sh --coverage
bash native/parity/preferences/verify.sh
```

Latest host integration: 102 native Rust tests, strict all-target Clippy and three-ABI debug APK/v1-v2 signature checks pass; nothing installed. JVM decoder owner coverage is 61/62 executable lines and 6/6 functions. Original UI/device and preferences integration gates remain open.

Login, layout, artwork, pure IME-session, secure-text policy, preference codec/host-runtime and JVM decoder owners each have an independent 95% line/function gate. Android callback/SDK effect execution remains separate and is not covered by host percentages. Neither merged averages,
test counts nor source fixtures prove branch/effect/device/pixel equivalence.

Desktop execution needs working X11/Wayland and OpenGL/EGL. X11 needs distribution-
matched XKB/XCB libraries; do not mix versions. Headless tests consume upload
batches without claiming actual rendering. Use `cargo run --locked --manifest-path
native/Cargo.toml --bin eh_native` only with a suitable display.

## Android debug APK

Linux host; SDK 37, Build Tools 37.0.0, NDK 29.0.14206865, Cargo and JDK/Gradle.
SDK resolves from `--sdk`, Android environment, or root `local.properties`.

```sh
python3 -B scripts/build_native.py --abi arm64-v8a --abi x86_64 --abi armeabi-v7a --jobs 2
# Single-ABI iteration:
python3 -B scripts/build_native.py --abi arm64-v8a
```

The separate `native/android/` AGP project packages framework DEX/resources and
the thin loader. Gradle invokes Cargo, but never builds the old app. Prefab
GameActivity glue is disabled because android-activity owns its patched glue.
Cargo/entry checks always run, including inherited workspace manifest/environment
changes; Cargo performs its own incremental compilation. JNI staging contains
exactly requested ABIs, never accumulated old outputs.

Builder validates target features, GameActivity exports, ELF ABI headers, real
DEX class definitions, Maven framework artifact hashes/class inventory, compile
SDK stubs and narrowly named D8-generated support classes. Unknown JVM origins
and old application classes are rejected. Class provenance/loader review are
build boundaries, not a proof of every third-party method's behavior.

APK identity, SDK levels, merged permissions, 16-KiB ZIP alignment and signatures
are checked before publishing `native/build/android/ehviewer-rust-bootstrap.apk`.
Only packaged native copies are stripped. Signing uses standard local Gradle
debug key, not production signing configuration. Current APK verifies v1/v2;
v3 is not enabled and is not claimed.

Nothing is installed or launched. ID `moe.tarsin.ehviewer.rust.prototype` cannot
upgrade the old app; its sandbox and current user edits remain untouched.

## Acceptance still blocked

Min SDK 23 / target 37 are build contracts, not device evidence. Prototype
requires OpenGL ES 3; original GPU/device coverage has not been established.
CJK fonts/IME, selection/autofill, TalkBack, lifecycle, system bars, predictive
Back, touch arbitration, rotation/restore, low memory and original UI screenshots
remain unrun. GameActivity + a compiled real AccessKit adapter does not waive them.

Source audit additionally confirms Android winit password-purpose/cursor-area no-ops.
The new raw TextEvent/TextAction route is connected only to a diagnostic page; it
bypasses neither secure/autofill policy nor Android font/bidi/inset gates. SDK native
text/action slots coalesce, lack focus IDs and are not transactional. Lone UTF-16
surrogates fail native decoding; mid-surrogate offsets have no exact renderer scalar
boundary. Original effective password action remains unverified. Characterize or
replace this platform seam before routing credentials or canonical session effects. AccessKit password semantics are not virtual autofill.
Do not enable a stock egui password field as the original secure-field replacement.
See [input gates](../docs/rewrite/ui-parity.md#android-input-gates).

Initial NativeActivity/DEX-free prototype is retired. Its earlier build/signature
and desktop screenshot records are historical, not evidence for current APK.
Separate Cargo workspace/lockfile keeps UI work isolated from the live JNI core.
Existing Rust owners remain canonical; no Kotlin fallback/backend selector exists.
