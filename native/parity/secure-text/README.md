# Secure-text source seam

Implemented metadata owner, not formal password widget or original UI acceptance.
No app, Android, winit, Cargo or IME wiring. `pub mod secure_text` exposes owner for
integration tests only; current renderer does not use it.

## Evidence binding

Immutable source snapshot: `858529fe3`. `source-binding.json` binds original
`SignInScreen.kt`, existing Material/resolved-dependency manifests, unchanged
Cargo files, cached AARs, their `classes.jar`, individual class bytes and complete
`javap -c -p -constants` class disassembly. All cache access read-only. All 17
bound classes use `sections: null` (whole class), not fragile line slices.
Existing AAR/jar/class identities unchanged; added mode and delegated codepoint
helper classes come from the same exact bound Foundation AAR. Source versions:

- Foundation Android `1.12.0-alpha02`.
- Material3 Android `1.5.0-alpha19`.
- UI text Android `1.12.0-alpha02` (TextRange units/predicates).

Cached source jars absent. Bytecode provides pinned source oracle; no online or
newer dependency substitution. `cases.tsv` holds 18 independently transcribed
source fixtures; Rust tests read expected masks/original-span instructions.
These fixtures do not execute Compose/JVM/Android. They prove predicates for
**supplied post-transformation metadata**, not IME-generated change lists.

## Frozen facts and discovery anchors

| Fact | Frozen evidence |
| --- | --- |
| Original hidden mode RevealLastTyped, toggle Visible; Password keyboard; onKeyboardAction signIn | Hash-bound SignInScreen.kt |
| Default mask U+2022 forwarded to Foundation | SecureTextFieldKt.javap default and Outlined lambda forwarding; BasicSecureTextFieldKt.javap |
| Exactly one changed range, absolute UTF-16 length 1, collapsed selection required | PasswordInputTransformation.javap `transformInput` offsets 0–52; TextFieldBuffer.javap; TextRange.javap |
| No origin, original replaced-range, composition, or focused predicate | Full `transformInput` disassembly. Single-unit paste/replacement/composition metadata eligible; actual edit-list generation external |
| Same index does not reschedule; different index schedules | `transformInput` offsets 53–90 |
| Timer signal collected with collectLatest; delay 1500ms then hide | SecureTextFieldController.javap `observeHideEvents`; SecureTextFieldController$observeHideEvents$2.javap |
| Reveal range.min stored in UTF-16 units, compared against codepoint ordinal | PasswordInputTransformation + SecureTextFieldController codepoint lambda + CodepointTransformationKt `toVisualText` |
| Astral insertion length 2 not eligible; astral prefix may reveal wrong following codepoint or no codepoint | Same unit/ordinal mismatch. Quirk preserved, not normalized |
| Every codepoint masked once; surrogate pairs consume 2 source units and 1 mask unit | CodepointTransformationKt.javap; CodepointHelpers_jvmAndAndroidKt.javap delegates to Character.codePointAt/charCount; StringsHelpers_jvmAndAndroidKt.javap delegates appendCodePoint |
| Reveal-enabled blur hides; Visible mode remains visible | BasicSecureTextFieldKt effective-reveal predicate, focus attachment conditional and mode transformation branches; controller focus lambda |
| Leaving effective RevealLastTyped hides through launched invalidation effect; enabling again reveals nothing retroactively | BasicSecureTextFieldKt predicate/effect wiring; BasicSecureTextFieldKt$BasicSecureTextField$2$1.javap |
| show_password >0 allows; <=0 forbids; resolver Exception uses true | BasicSecureTextField_androidKt$DefaultContentResolverForSecureTextField$1$1.javap |
| Mapped COPY/CUT preview commands consumed; five-callback toolbar overload nulls copy/cut, four-callback overload forwards | secureTextFieldModifier + DisableCutCopy toolbar bytecode. No claim that old overload currently executes; not global clipboard enforcement |

## Complete proof anchors (source-only)

Paths below relative to `oracle/`. Line anchors refer to whole saved classes;
bytecode offsets scoped to named methods, not mistaken for text line numbers.

- Effective reveal: `BasicSecureTextFieldKt.javap:621–648`, method
  `BasicSecureTextField-ltb6GB4`, offsets **1358–1429**. Only RevealLastTyped
  invokes Android platform flag; other modes set local boolean 23 to false.
- Focus attachment: same file `:869–876`, same method, offsets **1933–1952**.
  Boolean 23 selects controller focus modifier or empty Modifier, then `.then`.
  Callback behavior: `SecureTextFieldController.javap:124–133`,
  `focusChangeModifier$lambda$0`, offsets **0–19**: `!isFocused` calls hide.
- Mode identities: `TextObfuscationMode.javap:133–149`, static initializer
  offsets **11–29**: Visible=0, RevealLastTyped=1, Hidden=2. Companion getters
  in `TextObfuscationMode$Companion.javap:9–22` return corresponding fields.
- Each rendering branch: `BasicSecureTextFieldKt.javap:746–765`,
  `BasicSecureTextField-ltb6GB4`, offsets **1653–1704**: RevealLastTyped selects
  controller transformation; Hidden selects mask lambda; Visible matches neither
  and selects null transformation. Hidden lambda body at `:2171–2177`,
  `BasicSecureTextField_ltb6GB4$lambda$3$0`, offsets **0–12**, returns mask char.
  Reveal lambda at `SecureTextFieldController.javap:108–122`, offsets **0–30**,
  returns original only for matching ordinal, otherwise mask. Platform-disabled
  RevealLastTyped still selects controller transformation; effect below clears
  reveal index. No effective-mode normalization invented.
- Invalidation wiring: `BasicSecureTextFieldKt.javap:647–704`, main method,
  offsets **1427–1555**, boxes boolean 23, captures it in `$2$1` and launches
  effect. `BasicSecureTextFieldKt$BasicSecureTextField$2$1.javap:23–43`,
  `invokeSuspend`, offsets **32–52**, false invokes transformation.hide;
  true does not create reveal. Scheduler delivery remains runtime gate.
- Codepoint delegation: `CodepointTransformationKt.javap:34–73`, `toVisualText`,
  offsets **36–120**, calls codePointAt/charCount/appendCodePointX, advances
  UTF-16 source by charCount and ordinal by one. Complete helper bodies:
  `CodepointHelpers_jvmAndAndroidKt.javap:3–14`, codePointAt **0–5** and charCount
  **0–4** delegate to `java.lang.Character`; `StringsHelpers_jvmAndAndroidKt.javap:3–8`,
  appendCodePointX **0–5** delegates to `StringBuilder.appendCodePoint`.
  Isolated-surrogate fixtures exercise resulting JVM-contract unit rules in Rust;
  no Android libcore body/device execution substituted or independently certified.
- Toolbar overloads:
  `BasicSecureTextFieldKt$DisableCutCopy$copyDisabledToolbar$1$1.javap:19–30`,
  five-callback showMenu offsets **0–17**, replaces copy/cut callbacks with null;
  same file `:32–42`, four-callback showMenu offsets **0–16**, forwards all four
  callbacks unchanged. No assertion old overload currently exercised. Global
  clipboard restrictions remain unresolved.

## Ownership and safety

Caller supplies text length, exact post-transformation new ranges, selection,
platform result, authoritative focus notifications and monotonic time. Owner
stores only indices, mode/platform and timing metadata. UTF-16 source borrowed
for render planning, never retained or copied into plans. Debug for owner,
platform value and render spans redacted. No logging or credential I/O.

`apply_input` represents only events where source password input transformation
runs. Selection-only transformation invocation uses empty changes. Programmatic
edits that bypass transformation must not be relabeled as user edits.

Time passed on a newly revealed index models consumption of its hide signal,
not arrival of physical key/IME event. Current owner deadline determines hiding;
a stale callback merely advances time and cannot clear newer unexpired reveal.
No focus request, timer generation, channel, event coalescer or bridge authority
added. Runtime must establish correspondence before connecting this helper.

Invalid metadata, regressing clocks or overflowing deadlines return errors and
clear transient reveal. Last valid clock retained after regression. Error does
not override explicit Visible mode with fully-hidden UI. Empty/out-of-range
reveal ordinals produce no original spans. Render text guard beyond JVM maximum
length untested at runtime (would require >4GiB synthetic allocation); other
bounds paths exercised. Owner coverage includes this missed guard.

## Unresolved acceptance gates — no permissive defaults

- Formal password-widget semantics and rendering/pixel/interaction fidelity.
- IME applied-edit classification, original-range/coalescing/composition ownership,
  programmatic edit notification, selection delivery, focus and timer-consumption
  scheduling. Current bridge integration not attempted.
- Effective Done/default keyboard action/options, undo/redo and accessibility
  edit actions. Only original Password option/signIn callback fact frozen.
- Global clipboard, paste/import, drag/drop and every copy/cut escape path. Pinned
  preview/toolbar restrictions insufficient to declare all clipboard policy.
  Helper exposes no clipboard/edit permission API or allow-by-default decision.
- Android setting observer lifecycle/IO and platform exception delivery.
- Parent reports full 67-test suite, Clippy and three Android ABI APK builds
  passed. Not rerun here; these build gates do not prove widget/device semantics.
- Device/runtime oracle and original UI acceptance. Host helper does not meet them.

## Reproduce

```sh
bash native/parity/secure-text/check_host.sh --coverage
```

Runner uses std-only isolated rustc harness and private `native/build/secure-text`,
never Cargo or global caches for writes. Rust source/tests run with missing_docs,
unsafe_code and Clippy all+pedantic denied. Coverage filters tests and requires
owner-only line/function coverage >=95%. Source drift check regenerates whole
javap classes from pinned cache bytes. 24 checker regressions (22 rejection tests,
2 valid-output preservation tests) cover hashes/schema, malformed/missing/nonobject
JSON and invalid section bounds/content. Section selection rejects noninteger
(including boolean), reversed, zero, negative, out-of-range, empty or
instruction-free ranges before hashing; every selected range must contain
bytecode. Null means whole class; empty list never means whole class. Missing or
malformed JSON raises redacted DriftError with suppressed original exception,
carrying parent parser fix. Source-version changes require manual evidence
re-review; checker cannot update fixtures or fetch artifacts. Javap formatting
changes also fail closed.
