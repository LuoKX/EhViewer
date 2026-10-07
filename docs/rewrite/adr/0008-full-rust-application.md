# ADR 0008: Full Rust Application Target

## Status

Target accepted on 2026-10-07 after the operator explicitly selected
**full-stack rewriting, including UI**. Bootstrap implementation is separate
from production cutover. This does not claim the rewrite is complete.

## Decision

Project-owned application implementation will move to Rust: UI, navigation,
presentation, state/lifecycle orchestration, HTTP/session execution, persistence,
settings, downloads, background policy, codecs and archive/update handling.
Compose, Room, Ktor and Kotlin business/effect coordinators are not permanent
target owners anymore.

Android OS services, framework classes, SDK/NDK APIs, driver implementations and
build metadata remain external platform dependencies. JNI/NDK bridges may invoke
those services but must not hide application logic in Kotlin/Java or hand effects
back to the old app. "All Rust" refers to our application implementation, not
rewriting Android or claiming every third-party dependency is pure Rust.

Existing Rust owners remain canonical. In-process Rust callers use typed APIs,
not the Kotlin CBOR/JNI facade. New crates require a real implemented
responsibility; empty architectural placeholders are not added.

## Bootstrap choice, not production framework acceptance

Stage F0 initially evaluated pinned egui/eframe 0.36.2, OpenGL and Android
NativeActivity. Current bootstrap uses GameActivity with a thin Java loader.
Upstream has a NativeActivity example with API 23 metadata. Slint's documented
Android minimum is API 26, which would silently remove the current Marshmallow
baseline. No Android support level is dropped by this decision.

The first Android compile exposed a hard framework restriction: eframe 0.36.2
rejects AccessKit with NativeActivity. That limited/DEX-free configuration is
retired. Current GameActivity target explicitly enables accesskit_winit's Android
adapter and embedded framework DEX; eframe's generic feature alone otherwise
selects a no-op Android adapter. Target feature resolution and real ABI builds
are checked, but TalkBack/lifecycle/IME remain device gates. An all-Rust target
allows the minimum loading/OS bridge plus third-party/platform DEX, never hidden
project-owned Kotlin/Java business implementation.

The device gate must establish CJK fonts/IME, clipboard and selection,
accessibility/TalkBack, safe areas, touch/gestures, rotation, suspension/resume,
memory pressure and process restoration. A framework compilation or sample
window is not proof of those capabilities. If this gate fails, revisit the UI
framework before replacing production screens.

Upstream evidence reviewed for this choice:
- `https://github.com/emilk/egui/blob/main/examples/android_native_activity/Cargo.toml`
- `https://docs.rs/eframe/0.36.2/eframe/`
- `https://docs.slint.dev/latest/docs/rust/slint/android/`

Original UI/interaction fidelity was explicitly required on 2026-10-07. Stock
egui controls or simplified gestures cannot be accepted as equivalent to the
original Material3 UI. Source contracts, screenshot/interaction comparisons and
open gates are recorded in [UI fidelity](../ui-parity.md).

## Compatibility and cutover

- Existing installations, database/settings/backups, downloads and credentials
  are untouched by the bootstrap.
- The bootstrap has a distinct application ID, no production app data access,
  no network/storage permissions and a disposable debug key.
- Final package IDs, signing lineage, min-SDK/flavors, data upgrades and user
  workflows remain compatibility obligations until explicitly changed.
- Storage upgrades require copied historical fixtures, transactional migration,
  crash/power-loss tests and rollback/recovery evidence. Never experiment on the
  user's live database or delete old downloads.
- Do not manufacture success for unimplemented functions, move Kotlin behind a
  JNI call and call it a Rust port, duplicate existing Rust policy, or silently
  replace ICU Regex/Unicode, image/color or backup semantics.
- No runtime backend switch, parity run, or Kotlin error fallback is introduced.
  ADR 0007 continues to govern the live app until a complete application cutover.
  The new target supersedes only its permanent Kotlin-owner assumption.
- Remove old application implementation and dependency/build paths only after
  replacement feature, data and device gates pass.

Implementation slices and honest acceptance state live in
[Full Rust migration](../full-rust.md).
