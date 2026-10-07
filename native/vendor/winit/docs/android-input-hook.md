# Isolated Android raw-input hook

## Baseline / scope

Complete cached crates.io `winit 0.30.13` source seed, not a stripped fork:

- Published `.cargo_vcs_info.json` upstream Git SHA: `e9809ef54b18499bb4f2cac945719ecc2a61061b`.
- Cached `winit-0.30.13.crate` SHA-256: `a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`.
- All 217 upstream archive files remain present. Only `src/platform/android.rs` and `src/platform_impl/android/mod.rs` differ from archive bytes.
- Exact two-source-file patch: `docs/android-input-hook.patch` (zero-context unified diff; reapply with `git apply --unidiff-zero`). Zero-context avoids trailing-whitespace false positives from required blank context lines; independent reapply matches both modified sources byte-for-byte. Current source/patch SHA-256 digests and modification-notice provenance live in authoritative `docs/fork-provenance.json`; no duplicated stale digest here.
- Additions: this provenance note, patch, `docs/fork-provenance.json`, `tests/android_input_handler.py`, `tests/android_input_handler_api.rs`.
- Upstream manifests, lockfile, build script, licenses, and other sources unchanged. Upstream lockfile pins `android-activity 0.6.0`; isolated verification harness pins `0.6.1` without changing that lockfile.

## Public API / dispatch

Android-only `EventLoopBuilderExtAndroid` method:

```rust
fn with_android_input_handler<F>(&mut self, handler: F) -> &mut Self
where
    F: Fn(&activity::input::InputEvent<'_>) -> Option<activity::InputStatus>
        + Send
        + Sync
        + 'static;
```

Callback stored in cloneable `Arc`; wrapper Debug prints only `AndroidInputHandler`; equality uses `Arc::ptr_eq`. Existing attribute Clone/Debug/PartialEq derives retained. Default handler is absent.

Existing single `input_events_iter()` owner unchanged. Callback runs synchronously once per borrowed event on `android_main()` event-loop thread, before normal Motion/Key dispatch. No second input poller. Do not retain event borrows or borrowed contents; capture `AndroidApp`, context, and session separately. `None` delegates to original dispatch. `Some(status)` skips Winit dispatch and returns chosen status to activity iterator.

Motion/Key/volume handling and lifecycle unchanged. Default TextEvent/TextAction and future unknown variants explicitly return Unhandled without warning or raw formatting. No raw event trace added. No conversion to String/preedit, synthetic key events, IME reducer, secure editing, rendering, SDK configuration, effect policy, or authentication fallback.

Handlers must not log private raw input. Hook supplies no privacy policy for application callbacks. Unchanged upstream motion traces remain; `src/platform_impl/android/keycodes.rs:189` can warn with dead-key accent/Unicode characters on combination failure. This seam does not promise global secure-input logging behavior. Device/UI behavior unverified.

## Exact cached activity API audited

`android-activity 0.6.1` cached archive SHA-256: `0f2a1bb052857d5dd49572219344a7332b31b76405648eabac5bc68978251bcd`.

Relevant cached source:

- `src/lib.rs:1114-1172`: exclusive lending `AndroidApp::input_events_iter`, restricted to `android_main()` thread.
- `src/input.rs:1332-1336`: `InputIterator::next` takes `FnOnce(&InputEvent) -> InputStatus`.
- `src/game_activity/input.rs:28-33` and `src/native_activity/input.rs:549-554`: non-exhaustive borrowed InputEvent with MotionEvent, KeyEvent, TextEvent, TextAction variants.
- `src/game_activity/mod.rs:1066-1098`: invokes callback once per yielded event; **discards callback's InputStatus for every variant**. Returning Some suppresses Winit dispatch, but does not guarantee Android-level consumption or fallback. Hook does not alter activity implementation.

## Validation

Cached toolchain: `rustc 1.97.0-nightly (7c3c88f42 2026-05-14)`, `cargo 1.97.0-nightly (a343accce 2026-05-08)`.

1. Six stdlib Python structural checks pass:

   ```sh
   python3 tests/android_input_handler.py -v
   ```

   Verify single unchanged iterator owner; handler once before default; Android cfg and exact public bounds; cloned Arc storage/static Debug/pointer equality/retained derives; privacy-safe Unhandled text/unknown branch; unchanged original Motion/Key/volume dispatch (SHA-256 `7a803c8850284ef0fa392b89be84883da3a74da6a64cd4c8100ab9cec77062c6`).

2. Scoped formatter check passes, preserving upstream style instead of reformatting crate:

   ```sh
   rustfmt --check --edition 2021 --config skip_children=true,use_small_heuristics=Max,imports_granularity=Preserve,group_imports=Preserve,style_edition=2021,match_block_trailing_comma=true src/platform/android.rs src/platform_impl/android/mod.rs tests/android_input_handler_api.rs
   ```

3. Isolated offline Cargo harness checks actual modified Winit and public API probe for `aarch64-linux-android`, `android-game-activity/rwh_06`, exact activity `0.6.1`. Harness and generated artifacts remain outside seed in task-local `../validation-work/`; manifests/lockfile in source seed untouched. No main-project builder invoked. Android target check and Winit Android rustdoc both pass.

   Harness manifest (paths relative to task-local `validation-work/android-api/`):

   ```toml
   [package]
   name = "android-raw-input-api-check"
   version = "0.0.0"
   edition = "2021"
   publish = false

   [workspace]

   [features]
   reject-static-borrow = []
   reject-non-send = []
   host-x11 = ["winit/x11"]

   [lib]
   path = "../../winit/tests/android_input_handler_api.rs"

   [dependencies]
   winit = { path = "../../winit", default-features = false, features = ["android-game-activity", "rwh_06"] }

   [target.'cfg(target_os = "android")'.dependencies]
   android-activity = "=0.6.1"
   ```

   Commands from task cwd (parent of seed):

   ```sh
   cargo check --offline --locked --manifest-path validation-work/android-api/Cargo.toml --target aarch64-linux-android --lib
   cargo doc --offline --locked --manifest-path validation-work/android-api/Cargo.toml --target aarch64-linux-android --package winit --no-deps
   cargo check --offline --locked --manifest-path validation-work/android-api/Cargo.toml --features host-x11 --target x86_64-unknown-linux-gnu --lib
   ```

   Host non-Android cfg check also passes with harness `host-x11` feature and target `x86_64-unknown-linux-gnu`; it compiles Winit's unmodified host backend, not an Android runtime.

   Build environment follows read-only `scripts/build_native.py` target-specific environment: NDK `29.0.14206865`, `aarch64-linux-android23-clang`, corresponding clang++ and llvm-ar, target-specific CC/CXX/AR/linker; host compiler unchanged. CARGO_HOME, CARGO_TARGET_DIR, TMPDIR all task-local; cached registry read through private links, offline mode, automatic cache cleanup disabled. No root, SDK, or global Cargo-cache writes requested.

4. Actual Android API compile-fail probes reject static event borrows (`lifetime may not live long enough`, exit 101) and Rc capture (`E0277`, both Send and Sync failures, exit 101). Assertions on expected exit codes/diagnostics pass. Run harness check with `--features reject-static-borrow` or `--features reject-non-send`; these are negative tests, not runnable app handlers.

## Limits / handoff

Source/target compilation acceptance only. No Android device run, UI acceptance, IME behavior validation, or NativeActivity build claim. Independent reviewer gate and parent integration remain pending. Parent owns downstream callback/context/session capture and secure editor/IME policies.
