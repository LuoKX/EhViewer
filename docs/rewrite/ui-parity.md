# Original UI and interaction fidelity

Hard requirement confirmed 2026-10-07: full Rust implementation must preserve
original UI, interactions and feedback. A functionally similar redesign, default
egui theme or simplified gestures is not an acceptable replacement. Existing
production app/data stay unchanged until complete cutover gates pass.

## Acceptance contract

For every screen record original source anchors, compact/expanded layout, resource
qualifiers, locales/RTL, theme/wallpaper/AMOLED, density/font scaling, insets and
keyboard state. Reuse original assets, wording and component/navigation order.
Do not silently fix original quirks or add different reader shortcuts.

Source contracts and pure-state coverage are necessary but insufficient. Capture
original and Rust screenshots plus interaction traces on the same device/settings.
Check idle/pressed/focused/disabled/error/loading/empty/selected/modal states;
ripple, expressive shapes/motion, scroll position, animations and Back cancellation.
Test CJK composition, paste, selection, autofill, hardware input and TalkBack.
No tolerance or feature waiver has been approved. Differences stay open, not
renamed as completed migration.

## Current extracted sign-in seam

`native/src/login_state.rs` plus `native/tests/sign_in_screen.rs` freeze:
- active-job guard before username then password exact emptiness, no trimming;
- sticky errors until next submit reaches the field; error icon replaces toggle;
- original reveal-last-typed mode, not fully hidden password substitution;
- focus → spinner → request; failure hides spinner then awaits confirmation while busy;
- accepted/cancelled request retains spinner; obsolete completion hides it;
- guest termination changes neither errors nor spinner; registration/WebView not
  globally disabled by busy state;
- local callback IDs never substitute canonical session tokens or store credentials.

This module is extracted/tested but **not connected to an actual sign-in renderer
or Rust authentication runtime**. Guest outcome tests cover reducer termination,
not real task cancellation/exception wiring. Saved-screen field/state restoration
and destroyed-screen callback routing remain host responsibilities.

`native/parity/fixtures/signin/` binds current original presentation to source
hashes: 38 inputs, 12 locales, dimensions/qualifiers, logo/license and theme branches.
Exporter/tests under `native/parity/` are offline source oracles, not a runtime
Kotlin bridge. Numeric expanded threshold and runtime resource resolution remain
explicitly unresolved in that initial source export. Subsequent `material-source.json`
and `resolved-dependencies.json` bind original core/UI and default/marshmallow debug
runtime graphs: Compose1.12.0-alpha02, Material3 Android1.5.0-alpha19, window-core1.5.0
(expanded lower bound840dp), icons1.7.8. Release not queried. Resource selection is still
not reproduced. Live Git HEAD is not part of deterministic fixture bytes.

Source anchor: `app/src/main/kotlin/com/hippo/ehviewer/ui/login/SignInScreen.kt:83–400`.

## Rust presentation foundation

`native/src/signin_layout.rs` publishes widths before real widget/text measurement,
then places compact/medium/expanded source containers from caller-measured allocations.
Preserves modifier order, errors'16dp external padding, compact register/sign-in versus
expanded sign-in/register, padding-before-width128+8 slots, shared whole-row scroll,
physical effective system-bar insets and independent outer spinner.19 synthetic regressions
and source/bytecode provenance do not establish integer density rounding, real typography,
paint/touch regions or gestures. `compose-semantics.md` records exact measured boundary.

`native/src/signin_artwork.rs` renders original108dp logo from generated source coordinates,
colors and transforms. `scripts/export_signin_artwork.py --check` rejects source/code drift.
Prototype exposes only diagnostic asset view. Rasterizer/antialiasing equality unaccepted;
this is not a completed login page.

## Android input gates

Source audit of pinned native backend proves implementation gaps, not device acceptance:

- winit Android password purpose/cursor area remain no-ops. Minimal source-bound fork
  now provides one-owner raw TextEvent/TextAction callbacks, wired only to a diagnostic
  input page; no default-password substitution. Rust transport preserves UTF-16 spans
  independently of UTF-8 text; local tagged leases reject known stale local callbacks.
  Native events still lack originating focus IDs. GameTextInput coalesces text and
  overwrites pending actions; local snapshots cannot reconstruct chronological or
  atomic text/action pairs. Lone-surrogate decode failure and mid-surrogate scalar
  mapping remain blockers. Exact evidence: `native/parity/android-input/unit-contract.json`.
  Original Next/effective password action and exactly-once canonical effects still
  require acceptance; the password platform profile is not a device-tested secure field.
- Stock password mask differs from original1500ms reveal-last-typed policy, blur/selection/
  paste behavior and secure clipboard behavior even in Visible mode. Original undo/redo
  and system show_password baseline still need instrumentation. Never log real input.
- AccessKit password flag does not provide Username/Password ViewStructure/AutofillId or
  callbacks/session commit/cancel. Rust must own stable virtual IDs/values/focus/lifecycle.
- Bundled egui fonts are not Android system fallback. Current epaint has harfrust shaping,
  but mixed bidi/caret equivalence remains absent. Original font/palette inputs unresolved.
- Android content-rect/inset notifications ignored; adjustResize alone proves nothing.
  Reconcile surface resize with effective bars/IME once; do not subtract IME twice.

Required original/native tests: CJK preedit/commit/cancel, emoji/UTF-16 selections,
Next/Done/hardware Enter, secure reveal1499/1501ms and disabled-show-password, clipboard
all entry points, autofill + independent TalkBack, font-scale/CJK/mixed-direction caret,
bars/cutout/gesture navigation/IME animation/rotation. No physical/emulated devices run.

## Remaining screen / interaction map

Paths prefixed `ui/` are under `app/src/main/kotlin/com/hippo/ehviewer/`.
Inventory is source-based; none of these rows is product UI acceptance.

| Original anchors | Preserve / characterize |
| --- | --- |
| `ui/MainActivity.kt:183–270,283–402,409–528` | Drawer item/order, signed-out/configured/offline startup, restored stack, intent/notification/clipboard dedupe, window−56dp drawer width, blur 0→10, swipe and side-sheet collision, inset/back precedence |
| `ui/screen/MainNav.kt:17–43` | List URL → detail URL → reader-page route precedence; typed args and page index |
| `ui/screen/SearchBarScreen.kt:164–345` | Suggestions/history/tag insertion, submit normalization, delete confirmation, clear versus image search, expanded Back/drawer distinction, IME Search and non-touch focus escape |
| `ui/screen/GalleryListScreen.kt:214–391,480–624` | Tap detail, long-press gallery actions, list/grid, quick-search sheet/swipe cancel, category/language/advanced filters, refresh/range, date versus Toplist 1..200, Whatshot control exclusions |
| `ui/screen/GalleryDetailContent.kt:195–407,460–810,915–928`; `GalleryDetailScreen.kt:190–330` | Read resume 1-based label, partial/offline state, download/remove confirmation, disabled chips, adaptive previews, tags/copy/votes, ratings min .5, comments/favorites/new version, archive/torrent/browser/SAF/share failure feedback |
| `ui/screen/FavoritesScreen.kt:233–374`; `HistoryScreen.kt:111–162` | Local/remote categories and mutations, selection Back, batch actions, history clear confirmation, swipe delete and gallery action menus |
| `ui/screen/DownloadsScreen.kt:266–483,550–830` | List card → history+reader, thumbnail → detail, list long-press → selection, grid long-press → detail; labels/sheets, queues/start/stop, progress reset confirmation, selection FAB and sort/filter/scroll restore |
| `ui/settings/SettingsScreen.kt:35–80`; `EhScreen.kt:128–298` | Preference groups/routes, original defaults, conditional visibility/summaries, immediate theme/list effects; Filter/MyTags/UConfig need deeper fixtures |
| `ui/settings/DownloadScreen.kt:141–430` | SAF location/reset/cancel, originals/CBZ/metadata/concurrency/preload, recovery consent/progress/candidates and stalled/failure feedback |
| `ui/settings/PrivacyScreen.kt:46–78`; `AdvancedScreen.kt:131–326`; `AboutScreen.kt:79–130` | Secure screen/unlock/history, logs/cache/language/network, backup import/export/cancel, OS/default links, update/license flows |
| `ui/reader/ReaderScreen.kt:198–415` | Toolbar initially hidden, loader/window/orientation lifecycle, volume only when bars hidden, first-use overlay release, page long-press sheets, color-tab scrim/brightness restore, page indicator |
| `ui/reader/GalleryPager.kt:48–126`; `eu/kanade/tachiyomi/ui/reader/viewer/ViewerNavigation.kt:16–50` | Separate pager/webtoon navigation/invert modes, normalized tap zones and outside→MENU, mouse wheel conflation; exact geometry fixtures still needed |
| `ui/reader/PagerViewer.kt:75–271`; `DoubleTapZoom.kt:8–23`; `Utils.kt:45–120` | RTL XOR locale direction, horizontal/vertical swipe, zoom 1..5, smart/fit/crop/start position, delayed landscape fit, pan-before-page threshold, double-tap centroid/reset, first/last boundary and animation preference |
| `ui/reader/WebtoonViewer.kt:43–98`; `Utils.kt:104–120`; `SliderPagerDoubleSync.kt:25–78` | Whole-list zoom 1..3, side padding, continuous 15dp versus webtoon zero gap, .75 viewport tap scroll, long-press visible item at y, 1-based display versus 0-based loader, resume/mode-switch synchronization |
| `ui/reader/KeyEventHandler.kt:10–29` | KeyDown Up/PageUp previous, Down/PageDown next; volume gating/inversion; KeyUp/unhandled propagate; do not invent Left/Right/Space actions |
| `ui/theme/Theme.kt:41–98`; `ui/Theme.kt:29–65`; `core/ui/.../PredictiveBack.kt:45–124` | Android dynamic/wallpaper/fallback/AMOLED, expressive motion/typography, shared-axis transitions, Back progress/cancel rollback and RTL sign; Escape/pop alone is not equivalent |

Comments edit/reply/voting/BBCode, detailed settings subroutes, common gallery menus,
navigation-zone rectangles, archive passwords/corruption and save/share effects
require deeper inventories. Listed destinations are not blanket full coverage.

## Next end-to-end slices

1. Accept foundation on devices: original theme/insets/back, CJK IME, TalkBack,
   lifecycle/GPU/low memory. GameActivity/AccessKit compilation is not this gate.
2. Wire original sign-in renderer and canonical Rust session effects using above
   contracts; preserve WebView/browser/autofill and saved-screen behavior.
3. Copied offline download/settings fixture → original list/grid interactions →
   reader modes/gestures → Back/restored position/window state.
4. Catalog fixture → search/filter/detail/previews → reader; then downloads,
   persistent background effects, complete account/settings/localization flows.

No production UI, data format, support baseline or Kotlin owner is retired by
source fixture coverage. Full Rust completion remains pending.
