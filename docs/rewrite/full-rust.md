# Full Rust application migration

Scope confirmed 2026-10-07: **including UI**, not only portable core policies.
Target ownership is [ADR 0008](adr/0008-full-rust-application.md). The existing
Android application remains the production implementation until explicit cutover.
UI, interaction and feedback must match the original, not a redesign. The hard
[fidelity contract and source inventory](ui-parity.md) govern every screen.

## Current state

`native/` is an isolated Rust application bootstrap. It renders its own UI and
calls the existing gallery navigation parser in-process. It does not browse,
authenticate, read galleries, download, or access existing application data.
Those features remain **not migrated into the new application**, even where a
reusable Rust parser or policy already exists.

The earlier seven-category migration remains valid for the existing Kotlin app;
it does not establish a full-stack Rust application.

## Ordered slices and completion gates

| Slice | Application responsibility | Existing source anchors | Completion evidence |
| --- | --- | --- | --- |
| F0 | Rust UI entry, build/package, lifecycle feasibility | `ui/MainActivity.kt`, manifest; new `native/` | Host tests, real ABI APK checks, API 23/current Android startup, rotation/resume/back/IME/accessibility |
| F1 | Data and settings compatibility | `EhDB.kt`, `Settings.kt`, `core/data/` | Rust persistence against copied schema/history fixtures, settings/backup import, transactions, failed-upgrade recovery; no data loss |
| F2 | HTTP, sessions, credentials, authentication | `EhEngine.kt`, `SessionController`, `core/data/.../network`, login UI | Rust transport and session effects, offline HTTP contracts, cancellation, cookie/redirect/proxy/DNS/TLS compatibility, secure credential handling |
| F3 | Catalog and account UI | `ui/main/`, `ui/screen/`, `ui/settings/` | Search, gallery list/detail, previews, tags, votes, comments, BBCode, favorites/history, account/settings workflows in Rust |
| F4 | Reader and image pipeline | `ui/reader/`, `image/`, `coil/`, `gallery/`, `spider/` | Page/webtoon modes, gestures, zoom, prefetch/cache, GIF/WebP animation, JPEG/PNG/AVIF, colors/EXIF, memory and pixel fixtures |
| F5 | Downloads, recovery, archive and background execution | `DownloadManager`, `DownloadService`, recovery coordinators | Rust effect orchestration, queue persistence, SAF/FD, notification/service lifecycle, process death, cancel/resume, recovery and export |
| F6 | Update handling and other Android integrations | `AppUpdater`, `UrlOpener`, shortcuts, workers, permissions | ZIP/APK member selection and bounds, install handoff, intents/deep links/share, scheduling, biometric/security and permissions |
| F7 | Remaining compatibility and presentation rules | `EhFilter`, `STRICT_PAGE`, `EhUtils`, `ReadableTime`, `core/i18n` | Stored-pattern/Unicode behavior, localized resources/plurals/dates, title rules, accessibility and keyboard/TV behavior |
| F8 | Production upgrade and retirement | manifests, app/core/build/CI | Signed install-over-old/version upgrades, all E2E gates, ABI/flavor parity, old implementation/dependency removal and rollback evidence |

Source prefixes in the table refer to the current app's Kotlin tree unless a
module is explicitly named. The table is a work inventory, not a completion
percentage; F1–F8 are pending. Framework/device validation in F0 remains pending
until executed on supported Android devices. The retired NativeActivity prototype
had an AccessKit incompatibility. Current GameActivity target explicitly enables
and checks the real Android adapter, not the buildable no-op backend. Device
TalkBack/IME and original presentation gates remain open; accessibility is not waived.

### Rules for every slice

1. Characterize the current behavior before replacing it. Existing Rust logic is
   reused without a second implementation. Offline differential fixtures are
   permitted; production dual execution and fallback are not.
2. Separate pure business policy from Rust-owned effect runtimes and Android
   OS calls. Java/Kotlin shims cannot keep the migrated implementation.
3. Critical owners retain independent 95% line/function gates. Add protocol/data
   compatibility tests and effect order/cancellation/partial-failure tests.
4. Rust UI tests and host rendering are not Android lifecycle, gesture, IME,
   accessibility, background-service or hardware-decoder evidence.
5. Network, storage and codec dependencies are selected at their slice, not
   installed speculatively. Existing data formats are not silently abandoned.
6. Record failed attempts and unrun gates. No "full migration" claim for a shell,
   pending-feature panel, imported crate, successful compilation or APK alone.

## Required inventory before production cutover

- All navigation destinations and interactions, including web-based login,
  gallery comments/tagging/rating, torrent/remote archive flows and image search.
- Local/remote favorites, history, quick searches, labels, artist indexing,
  backup import/export and database/schema/settings upgrades.
- Reader modes, gestures, input devices, page selection, original images,
  color controls, animated formats and archive passwords/corruption handling.
- Persistent downloads/recovery, quotas, cancellation, process death, SAF
  provider permission loss, progress notifications and scheduled daily work.
- Localization/plurals, CJK font coverage and input, accessibility, system bars,
  orientation, multi-window, battery/memory behavior, permissions and security.
- Existing API 23/API 26 flavors, arm64/x86_64/ARMv7 packaging, production
  identity/signing, distribution/update and complete-version rollback.

## Initial F0 NativeActivity execution record (historical)

Executed on 2026-10-07. Logs and the diagnostic screenshot are under
`native/build/` and are not committed.

| Gate | Result |
| --- | --- |
| New standalone Rust tests | 7 passed |
| Existing Rust workspace, all features/integration/doctests | 575 passed; no existing core source/lockfile changes |
| Python script suite | 19 passed, including 7 new packaging contracts |
| Host and Android Clippy | Host plus arm64-v8a, x86_64 and armeabi-v7a, all targets, `-D warnings` passed |
| Rustfmt, rustdoc, Python style, rewrite naming | Passed; rustdoc `--no-deps` with `-D warnings`; Ruff includes `S314` with a justified trusted-manifest test exception |
| Scoped active LSP | 6 changed source/test files confirmed clean, 0 errors; not a repository-wide scan |
| Actual Android debug APK | Three ABIs packaged, entry exports present, legacy business JNI absent, exact ELF ABI headers and no DEX verified |
| Binary/package layout | ARM64/x86_64 ELF LOAD alignment 16 KiB; ARMv7 4 KiB; APK ZIP alignment checked at 16 KiB |
| APK metadata/signature | Isolated prototype ID, min SDK 23, target 37; v1/v2/v3 signature verification passed |
| Desktop rendering | Real Xvfb/OpenGL run produced `native/build/bootstrap.png` and exited successfully; not an Android device test |
| Android runtime/device execution | NOT RUN: `adb devices` lists no connected devices |

Artifact: `native/build/android/ehviewer-rust-bootstrap.apk` (debug-only,
not installed). Native debug symbols remain in Cargo outputs; only packaged
copies are stripped. Existing user changes to `.gitignore` and
`DownloadRecoveryCoordinator.kt` have unchanged before/after SHA-256 values.

Failed attempts are retained in local logs: the headless UI test initially failed
because its unused texture-upload batch was not explicitly consumed; that test
now consumes the batch without pretending to render it. Android's first compile
exposed the AccessKit/NativeActivity blocker described above. Android Clippy
required explicitly declaring the entry's actual Rust ABI, not changing it to C.
The desktop host initially lacked XKB X11 libraries; an older auxiliary XKB
library then caused a segmentation fault. Extracting distribution packages locally
and matching the auxiliary library to the host's installed XKB version produced
a successful render; no system packages were installed or replaced.

That initial F0 artifact was **buildable but not accepted as a production UI
foundation**. Its NativeActivity/DEX-free configuration is retired, and the
current artifact is described below. Device gates, including CJK/IME, lifecycle,
GPU compatibility, system bars and low memory, still must pass. That initial
prototype manifest required OpenGL ES 3; it did not prove preservation of the
old app's complete GPU/device baseline. No new 95% coverage, Kotlin/Gradle test,
full product feature, device or performance acceptance was claimed at that stage.

## GameActivity / original-UI contract record (initial snapshot)

This table records the first GameActivity integration, not the latest test counts.
Current foundation and per-feature continuation are tracked in [progress](progress.md).

Continued on 2026-10-07 with operator-approved isolated parallel subagents.
Original UI/interaction fidelity is a hard requirement, not a redesign allowance.

- `native/android/` is a separate AGP 9.2.1 application with one thin Java loader,
  GameActivity 4.4.0 and the existing isolated prototype ID.
- eframe's generic AccessKit flag alone compiled a no-op Android adapter; independent
  review found this P1. Android now explicitly enables
  `accesskit_winit/accesskit_android`; locked adapter 0.7.5 includes embedded DEX.
  Build checks resolved per-target features and rejects a no-op/NativeActivity configuration.
- Cargo always runs its validation/staging hook. Inherited Rust workspace manifest
  is tracked; Cargo, not Gradle's partial file list, owns target/environment invalidation.
- DEX class definitions are bound to SHA-256-recorded Gradle framework artifacts,
  compile-SDK class names, generated resource/owner-linked synthetic classes and
  five exact pinned-D8 support templates. Unknown namespaces/owners and legacy
  app classes fail. This is provenance/ownership evidence, not a proof of arbitrary
  third-party bytecode behavior.
- Original sign-in interaction reducer lives in native crate with 18 regressions.
  It is not wired to authentication or a product SignIn renderer.
- `native/parity/` freezes 38 source inputs, 12 locales, original logo/license,
  layout/qualifiers, errors and theme references. Live HEAD is reported separately;
  unrelated commits do not break deterministic fixtures. XML is bounded and
  DTD/entity declarations rejected.

| Initial integration gate | Evidence / status |
| --- | --- |
| Native Rust tests | 7 previous bootstrap + 18 login regressions pass |
| Pure login owner coverage | LLVM: 99/99 lines, 7/7 functions; independent 95% gate passes; branch/runtime coverage not claimed |
| Python contracts | 16 packaging + 5 coverage-gate + 30 original-source baseline tests pass |
| Android APK | ARM64, x86_64, ARMv7 rebuilt with real Android adapter; exports, ABI headers, DEX definitions/origins, identity, SDK/permissions, ZIP alignment pass |
| Signing | Current Gradle debug APK v1/v2 verification passes; v3=false, unlike historical manual packager |
| Host + Android Clippy | Host and all three ABI targets, all targets, `-D warnings` pass |
| Scoped active LSP | 11 changed source/test/bridge files confirmed clean, 0 errors; not a repository-wide scan |
| Static GameActivity binding | `javap` confirms `mSurfaceView` field descriptor matches accesskit_winit's JNI lookup; not runtime/TalkBack evidence |
| ELF layout | Current ARM64/x86_64 LOAD alignments 16 KiB; ARMv7 4 KiB |
| Independent fix review | Third pass `1bdc7242-e5e0-4555-b7f2-7ea5d5dc430d`: OK with notes for this non-product foundation only |
| Real UI / device gates | NOT RUN: no connected adb devices; no original-versus-Rust screenshot, IME, TalkBack, gesture or performance acceptance |

Current artifact remains `native/build/android/ehviewer-rust-bootstrap.apk`,
debug-only and not installed. It is not a production replacement. Renderer still
shows diagnostic screens, not original product pages. GLES3 device compatibility,
all F1–F8 responsibilities, actual login effects, locale/resource resolution,
original Material widgets/motion and Android lifecycle/restore remain open.

Failed compile/classpath, AGP manifest-SDK, artifact-name, modified-UTF-8 dump and
origin-inventory attempts are build/debug evidence, not suppressed or called
accepted UI. Independent review rechecked and closed the two P1/two P2 findings
after fixes; no remaining foundation blocker was found. Final user-file hashes,
source gates and actual APK checks pass. This does not close device/product gates.
The existing core/legacy Kotlin owners and user changes remain untouched.
