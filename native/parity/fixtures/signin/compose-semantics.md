# SignIn geometry provenance / conditional constraint characterization

## Original source, read-only

Reference HEAD inspected: `cd07574903bdf83e79f28975359c34ef98908ad6`.
Working-tree bytes used, not claimed clean HEAD bytes. Baseline JSON fully parsed:
`native/parity/fixtures/signin/baseline.json` (38 sources, 12 locales).

| Rule / derived quantity | Original anchor |
|---|---|
| Explicit compact/medium versus expanded; no numeric breakpoint | `SignInScreen.kt:207`; `core/ui/.../util/WindowSizeClass.kt:10` predicate only |
| Preferred field width; non-error-only bottom padding16 | `SignInScreen.kt:152..202`, especially158,177 |
| `fillMaxSize.verticalScroll.systemBarsPadding.padding(keyline)`; Column horizontal center/top default | `SignInScreen.kt:208..216` |
| Logo padded on each side by keyline, no forced logo size | `SignInScreen.kt:217..225,321..328` |
| Compact warnings widthIn(single_max_width), added top24 then12 | `SignInScreen.kt:226..239` |
| Compact spacer keyline then primary Row top keyline | `SignInScreen.kt:243..249` |
| Compact primary register→sign-in, two weight1 children, each horizontal pad4; half available width BEFORE padding | `SignInScreen.kt:250..263` |
| Compact secondary webview→guest, Row horizontal pad4, gap8, weight1 each; `(max(bodyWidth-8,0)-8).max(0)/2` widget widths | `SignInScreen.kt:264..300` |
| Expanded ONE scrolling Row, vertical center; caption then remaining-width form | `SignInScreen.kt:304..342` |
| Expanded caption preferred480 BEFORE caption keyline padding; warnings cap360, no added vertical gaps | `SignInScreen.kt:309..337` |
| Expanded form field→field→spacer8→primary→spacer8→secondary | `SignInScreen.kt:339..392` |
| Expanded primary sign-in→register, secondary webview→guest; pad4 BEFORE width128 yields136 wrapper,128 widget when unconstrained | `SignInScreen.kt:344..380` |
| Spinner outer Box center, outside either scroller; size not specified here | `SignInScreen.kt:205,396..399` |
| Default keyline16, single_max260, caption480 | `app/src/main/res/values/dimens.xml:19,22,23` |
| sw600 keyline24, single_max480, caption inherited480; NOT expanded breakpoint | `app/src/main/res/values-sw600dp/dimens.xml:19,20` |
| Original logo positive intrinsic108×108 | `app/src/main/res/drawable/ic_launcher_foreground.xml:19,20` |

All Kotlin anchors above use `app/src/main/kotlin/com/hippo/ehviewer/ui/login/SignInScreen.kt` unless qualified otherwise.
Tests use synthetic widget allocations; their56/60/40/etc heights are NOT source Material metrics.

## Dependency binding gate — debug runtime graphs resolved

`gradle/libs.versions.toml:19,78` selects `androidx.compose:compose-bom-alpha:2026.05.00`;
foundation alias at79 is unversioned. Cached Compose source JARs absent. No lockfile,
resolved original dependency graph, or cached BOM POM found to bind original selection
conclusively to cached binaries. No Gradle resolution, installation or network attempted.

Supervisor approved local AAR extraction + `javap` as **conditional** evidence.
Original worker handoff left cache binding conditional. Parent subsequently ran offline
`:core:ui:dependencies --configuration androidRuntimeClasspath` and both
`:app:dependencies --configuration defaultDebugRuntimeClasspath` / `marshmallowDebugRuntimeClasspath`.
All three reports contain zero FAILED dependencies and select these exact characterized versions.
`resolved-dependencies.json` records command/configuration, input/log hashes and selected versions.
Material3 Android 1.5.0-alpha19, window-core1.5.0 and icons1.7.8 were also bound. Release not queried.
This resolves debug-graph binding, not widget, density, renderer or device parity. Historical scout
notes above describe their earlier read-only boundary, not current parent verification.

## Cached artifacts actually characterized

Under `/home/ngnl/.gradle/caches/modules-2/files-2.1/`:

| Coordinate / AAR path suffix | SHA-256 of actual AAR |
|---|---|
| `androidx.compose.foundation:foundation-android:1.12.0-alpha02` / `androidx.compose.foundation/foundation-android/1.12.0-alpha02/edbcfaa3ef1e5227f1fa8e4bcb4b1ce6165d3ce0/foundation.aar` | `74344537a4e46d1dd3c3319710d6950e17b3fd465c3f11f364fa75c20282d936` |
| `androidx.compose.foundation:foundation-layout-android:1.12.0-alpha02` / `androidx.compose.foundation/foundation-layout-android/1.12.0-alpha02/7754ad6f7643e45b32673d757deb1b286b909af3/foundation-layout.aar` | `3f23a4a8433dbea0117e4623d4b45f077ac7776f99c03ac82928657fccec3753` |
| `androidx.compose.ui:ui-android:1.12.0-alpha02` / `androidx.compose.ui/ui-android/1.12.0-alpha02/da15a42077db8b31b91432971c657c10a08963c2/ui.aar` | `5c0ee366839b924bf7fb03f9d1d1997817833a62be3f7112674d7c28271a96b7` |
| `androidx.compose.ui:ui-unit-android:1.12.0-alpha02` / `androidx.compose.ui/ui-unit-android/1.12.0-alpha02/d5834975ac38875fde3c554ec07fae5c64d0b699/ui-unit.aar` | `382d86b9299700074b1b9913cc030cdb706cf312090a7b05768eec7c916f3aa4` |

Extracted ONLY `classes.jar` into isolated cwd with Python `zipfile.ZipFile(aar).read('classes.jar')`.
Inspected using `javap -classpath <isolated classes.jar> -c -p <class>`.
Scratch binaries/raw disassembly subsequently removed; no dependency/vendored code shipped.

### Key classes, methods and bytecode offsets

Offsets refer to `javap` bytecode instruction offsets, not original Kotlin line numbers.
Class packages: `androidx.compose.foundation` for ScrollNode/ImageKt;
`androidx.compose.foundation.layout` for layout classes; `androidx.compose.ui` for UI classes.

* `ScrollNode.measure-3p2s80s`: offsets27..29 use `2147483647` max height;
  56..65 `Constraints.copy(..., mask=5)` preserves **minWidth/minHeight** while replacing
  maxWidth/maxHeight; 70..78 measures child; 80..116 bounds viewport and subtracts
  childHeight-viewportHeight; 144..171 updates shared max/viewport.
  `measure_3p2s80s$lambda$0`: offsets4..65 clamps state value0..max;
  67..84 chooses `-scroll` for reverseScrolling=false; 86..114 selects vertical offset.
  Therefore `contentHeight=max(naturalBodyHeight,viewportHeight-barsY-2*keyline)+barsY+2*keyline`
  in non-overflow domain; ONE `maxScroll=max(contentHeight-viewportHeight,0)`.
  Insets/keyline padding lie downstream and translate with this child.
* `FillNode.measure-3p2s80s`: bounded-axis min/max assigned same incoming maximum,
  using fraction and constrain range. Original fillMaxSize therefore retains viewport
  min height before scroll removes only vertical maximum; fillMaxWidth form consumes
  remaining constrained Row width.
* `PaddingNode.measure-3p2s80s`: offsets0..44 round each padding edge;46..62 measure
  under `Constraints.offset(-horizontal,-vertical)`;64..90 constrain sum of
  child+padding. `measure_3p2s80s$lambda$0`:7..28 uses `placeRelative(start,top)`.
  Padding offsets remain full even when allocation clamps below total padding:
  zero-sized child may sit outside wrapper. Rust returns unclipped allocation rects.
* `InsetsPaddingModifierNode.calculateInsets`:0..8 unions ancestor consumption
  with requested insets. `measure-3p2s80s`:0..138 subtracts ancestor-consumed physical
  left/top/right/bottom from consumed union;154..202 offsets constraints and constrains
  measured child+insets. `measure_3p2s80s$lambda$0`:0..7 uses **absolute** `place`.
  Rust caller supplies already-effective physical insets; no OS/IME inference.
* `ui.unit.ConstraintsKt.offset-NN6Ew-U`:0..26 floors adjusted minWidth at0;
  26..75 adjusts finite maxWidth with same floor but preserves infinite maximum;
  76..151 does same for heights. `constrainWidth-K40F9xA` and
  `constrainHeight-K40F9xA` clamp extents within incoming min/max. Thus extreme
  padding reduces child constraints to0 while placement still retains full padding.
* `SizeNode.measure-3p2s80s`:13..26 branches on `enforceIncoming` and invokes
  `Constraints.constrain(incoming,target)`. `getTargetConstraints-OenEA2s` supplies
  equal min/max for preferred `width`; max-only for `widthIn`. No requiredWidth overflow.
* `RowColumnMeasurePolicyKt.measure`:186..253 computes
  `remainingMax=max(mainAxisMax-fixedSpace,0)` for each unweighted child, source order;
  282..365 adds measured size/limited spacing and max cross-axis size.
  398..461 weighted target uses bounded mainAxisMax, subtracts spacing, divides remaining
  by total weight; 505..528 and708..725 round weighted shares/remainder in actual runtime.
  1073..1118 computes main and cross extents using max with incoming **min** extents.
  This establishes caption-first remainder, first expanded136 slot before clamped second,
  and expanded Row height at least viewport height minus nested padding.
* `RowKt.Row`: default arrangement at36=`Start`, default vertical alignment at50=`Top`.
  `ColumnKt.Column`: default arrangement at36=`Top`.
  `RowMeasurePolicy.populateMainAxisPositions`:16..25 passes inherited layout direction.
  `getCrossAxisPosition`:40..50 delegates vertical alignment to each child's measured height.
  Caption/form centered independently in same Row, not independently scrolled.
* `Arrangement$Start$1.arrange`:0..29 calls left placement for LTR, right placement
  with reversal flag for RTL. `Arrangement$Center$1.arrange`:0..30 calls center
  with reversal=false/true. `Arrangement$SpacedAligned.arrange`:29..46 chooses RTL;
  RTL107..141 clamps positions/non-overflow spacing; LTR210..250 uses
  `min(space,remaining)` after each child. Tiny-width gaps clamp, not overrun.
  Rust mirrors continuous logical start positions and preserves source array order.
* `ImageKt.Image(Painter,...)`:411..414 default alignment Center,426..432 default
  ContentScale.Fit. `ui.draw.PainterNode.modifyConstraints-ZezNO4M`:207..220 clamps
  intrinsic dimensions,223..282 scales them,284..396 clamps scaled min dimensions.
  `calculateScaledSize-E7KxVPU`:329..346 applies ContentScale;349..352 returns zero
  size for zero destination width/height. `ui.layout.ContentScale$Companion$Fit$1`
  calls `ContentScaleKt.computeFillMinDimension-iLBOSCw`; that helper divides
  destination/source dimensions at73,151 and calls Math.min at158, establishing intrinsic
  aspect shrink after logo padding. Zero intrinsic dimensions themselves remain
  explicitly unsupported, not guessed.
* `ui.BiasAlignment$Vertical.align`: subtraction/division by2 then bias factor;
  offset23 invokes Math.round. `ui.BiasAlignment.align` likewise rounds positions.
  Rust intentionally uses continuous division, not claimed integer placement parity.

## Boundaries not supported/proved

* Release-variant binding unqueried; debug bindings recorded above. No APK/runtime parity claim.
* Density conversion, per-edge dp rounding, integer weighted remainder allocation,
  integer alignment, finite packed Compose constraints: not emulated. Even caller-rounded
  widget heights cannot close padding/alignment rounding gap. No pixel equivalence claim.
* Supporting-text exact offset/rectangle: unsupported; widget total extent + support text
  height supplied by caller. Error bool explicit. No support height is added twice.
* Text wrapping, label/trailing icon measurement, minimum interactive allocation and
  Material widget dimensions: caller-measured. Wrong-width heights cannot be detected.
* Actual OS consumed insets, resource qualifier selection, viewport/width class: caller inputs.
* Logo paint path/rasterization, theme/localization, gestures, accessibility, focus,
  IME/autofill, spinner animation and interactions: no implementation changes here.
