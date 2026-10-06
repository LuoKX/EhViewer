# Whole-repository redundancy audit — 2026-10-06

Baseline: `754131dfb` (`feat(core): migrate P4 portable policies to Rust`).

## Scope and deletion rule

Operator selected confirmed redundancy only: preserve existing user-visible
features, compatibility behavior, database schemas/migrations, JNI contracts and
useful regression/golden-generation tools. No feature deprecation or dependency
upgrade is part of this cleanup. Git submission requires separate operator approval.

Repository inventory covers app, all four core modules, benchmark, build logic,
Rust workspace/vendor integration, Android resources, CI and fixture tools.
Tracked implementation inventory before cleanup: 535 Kotlin, 249 Rust, six Java,
one C/C++ translation unit, 65 XML and ten Python files, plus Gradle Kotlin DSL.
CodeGraph indexed 753 files / 17,303 nodes / 35,363 edges before changes.

This is a repository-wide structural/reference/dependency/build audit, not a
claim that every branch has been exercised on Android. No-incoming-edge queries
are candidate generation only. Every deletion also received literal-reference
and owning-path review; callback, codegen, serialization, overload, template and
native entry points are not classified as dead merely because graph edges are
missing. Generated classes and third-party implementation internals are not
bulk-deleted.

## Functionality inventory and disposition

| Area | Owners reviewed | Disposition |
| --- | --- | --- |
| Login, cookies, sessions, site selection, WebView login | Manifest, application, client/session, Ktor/OkHttp/Cronet, auth bridges | Preserved; unused historical exception classes removed |
| Browse, search, previews, metadata, titles, tags/translations | Client/parsers, gallery bridges, list/detail/search screens, Rust net/domain | Preserved; unused Kotlin URL/cache constants and getters removed |
| Favorites, history, filters, comments, rating/votes, torrents | Gallery/library bridges, client, UI, core models/DAO | Preserved; unused vote-tag DTO, favorite helper and obsolete XML toolbar removed |
| Online/local/archive reading, paging, image buffers/caches | Reader/gallery/spider, image/Coil, native decoder bridges | Preserved; no decoder, fetch, lease or memory-policy rewrite |
| Download queue/progress, archive/ComicInfo preparation | Download/spider, archive bridges, Rust archive/domain | Preserved; XML serialization runtime dependencies retired |
| Recovery scanning, identification, candidate promotion, cleanup | Recovery coordinator/lookup, scan/storage bridges, recovery DAO, SAF adapters | Preserved; unused failed-query entry point, facade candidate-ID accessor and regex removed |
| Settings, preferences, privacy/biometric, language, backup, updater | Settings, core preferences/database/i18n, settings screens | Preserved; unused preference factories and thread-check helper removed |
| Compose/navigation, side sheets, framework components | App/core UI, generated destinations and callback implementations | Preserved; unused drawer-density state/write/check removed |
| Notifications/background work, daily/news checks, shortcuts/app links | Manifest/services/WorkManager, dailycheck/news, activities | Preserved; Android roots remain even without ordinary callers |
| Packaging, native exports, baseline profiles, developer gates | Gradle/build logic/CMake, benchmark, manifests, CI/scripts | ARM64 default and ABI overrides preserved; unused plugin declaration and redundant compiler arguments removed |

## Confirmed removals

Six complete obsolete files removed:

- `app/src/main/kotlin/com/hippo/ehviewer/client/exception/OffensiveException.kt`
- `app/src/main/kotlin/com/hippo/ehviewer/client/exception/PiningException.kt`
- `app/src/main/kotlin/com/hippo/ehviewer/client/parser/VoteTagResult.kt`
- `app/src/main/kotlin/com/hippo/ehviewer/util/CollectionsExtensions.kt`
- `app/src/main/kotlin/moe/tarsin/coroutines/SuspendCollections.kt`
- `app/src/main/res/menu/context_comment.xml`

Each Kotlin file has no consumer outside its own dead implementation. The menu
and all seven IDs have no source/resource consumers; Android lint also reports
`UnusedResources`. Compose BBCode formatting continues to use the existing
format strings and Rust planning; no formatting feature or localized string is
removed. Tag voting continues through its typed Rust envelope rather than the
unused server-response DTO.

Other removed declarations/subtrees:

- `EhUrl`: `FAV_PATH`, `WATCHED_PATH`, `apiUrl`, `popularUrl`, `imageSearchUrl`,
  their six getter-only URL constants, `getArchiveUrl`, and
  `ParametersBuilder.addQueryParameterIfNotBlank`. Active Rust request plans and
  the remaining popup/torrent/tracker/wiki URL paths are unchanged.
- `EhCacheKeyFactory`: `URL_PREFIX_THUMB_E`, `URL_PREFIX_THUMB_EX`.
- `FavListUrlBuilder.isValidFavCat` and `LibraryRuleBridge.LOCAL_FAVORITED_SLOT`.
- `DownloadRecoveryCoordinator.IMAGE_ENTRY`; the active platform-observed page
  regex and Unicode filename behavior remain unchanged.
- `DownloadRecoveryEhLookup.markQueryFailed`; existing called lookup/failure
  paths and Rust failure-policy tests remain. No retry/cancellation behavior is
  changed.
- `EhDB.getDownloadRecoveryCandidate` only. The DAO `load(candidateId)` query is
  retained: the promotion transaction calls it internally, despite graph
  resolution attaching its generic `load` call to another Kotlin declaration.
  Listing/observation, source-key reuse and promotion transactions remain. No
  table, key, migration or persisted candidate data is removed.
- Preference factories `floatPref`, `doublePref`, `stringSetOrNullPref`,
  `byteArrayPref` and their now-unused imports. No actual preference or stored
  key is removed.
- `checkMainThread` only; `checkNotMainThread` and its platform implementation
  remain active.
- `isAtLeastN` only; the used API-level guards remain.
- Drawer `requireDensity`, its never-read `density` state, unused imports and
  sole state assignment. Local density still drives dimensions; anchor updates,
  animation, drag handling and saved drawer state remain.
- Rust `jvm::input_body`, an unreferenced forwarding alias for
  `codec::input_text`. The codec owner and JNI parser/CBOR operations remain.

Dependency/build cleanup:

- Remove `io.github.pdvrieze.xmlutil:serialization` and `serialization-io`, their
  version/catalog entries and bundle membership. There are no XMLutil imports
  in production, tests or core. ComicInfo XML is already Rust-owned. CBOR, JSON
  and JSON I/O remain used and retained.
- Remove unapplied `com.android.library` root/plugin-catalog entry.
  `com.android.lint` is **not** redundant: the KMP convention explicitly applies
  it and continues to do so.
- Remove `-Xcontext-parameters` and
  `-Xannotation-default-target=param-property`, which the pinned Kotlin 2.4
  compiler explicitly diagnoses as redundant. Default behavior matches the
  prior settings; experimental collection literals and JVM `when` lowering
  arguments are unchanged.
- Existing CI retired-source check now rejects XMLutil imports and runtime
  catalog artifacts, alongside the retired Kotlin diff implementation.

Touched Kotlin files follow the existing Google-style ratchet. Drawer and
preference-file reformatting is mechanical beyond the deletions above.

## Explicitly retained apparent duplicates/dead roots

- `default` minSdk 26 and `marshmallow` minSdk 23 are both supported. Default-only
  lint marks some API-26 checks and v25/v26 resource folders obsolete, but they
  remain required by the API-23 variant. No resource folder merge, permission
  removal, flavor removal or ABI-override removal is made.
- `ByteBufferSource.unmarshalParsingAs` is still called by 25 migrated parser
  paths. Its old-looking name does not make its direct-buffer/envelope transport
  a Kotlin business fallback.
- `Settings.retiredRustCoreFlags` is stored-key cleanup for previous versions,
  not a set of live rollout switches. Keep this data migration.
- Room migrations/type converters, Kotlin destructuring operators, Compose
  destinations/previews, Coil factory callbacks, Drawable methods, Cronet
  upload callbacks and Android lifecycle/manifest roots have implicit consumers.
- WebView's publicly exposed navigator surface and asynchronous client hooks
  are not trimmed from graph-degree statistics; copied API integration is kept
  outside the proven-dead private-helper cleanup.
- Kotlin HTTP/SAF/Room/Compose/image effects implement Android operations; Rust
  plans/codecs implement different responsibilities. They are not duplicate
  implementations to delete by language or filename.
- All seven Rust packages have source references for their direct dependencies.
  No Rust dependency or archive format/codec is proven redundant here. Bounded
  archive validation/reference readers, vendored safety patches, JNI manifests,
  compatibility fixtures, oracle generators and coverage masks remain useful.
- C link anchor/CMake and native export retention still assemble the final
  Android shared library; there is no legacy application C/C++ algorithm to
  delete from that translation unit.

## Acceptance

Completed native/source checks:

- Rustfmt, workspace/all-target/all-feature Clippy and warning-clean Rustdoc.
- 517 Rust tests/doctests pass.
- Exact host export manifests: 90 Java JNI exports plus `JNI_OnLoad` unchanged.
- Four real host JNI fixtures pass: BBCode, ComicInfo policy, storage rules,
  recovery scan (including bounded buffers, stale handles and scoped closes).
- Rewrite naming and four Python gate/mask tests pass.
- Rust LSP: no diagnostics. Configured `kotlin-lsp` command is unavailable;
  install it or update its command in `pi-lsp.json` for Kotlin LSP validation.

Completed Android/build checks:

- Default and Marshmallow each pass 299 JVM tests (598 executions), with no
  failures/errors/skips. Both instrumentation source sets compile.
- Both flavor lint tasks pass with no errors or `UnusedResources`. Remaining
  findings are 54 Default / 46 Marshmallow warning/info items, mostly dependency
  update advisories and existing platform guidance; no unrelated upgrades are
  applied.
- App/core common/core data/core UI Spotless checks pass. Drawer remaining code
  tokens/string literals match the baseline after only the declared deletions
  and removal of the now-unused state import.
- ARM64 Default Release builds. All 107 Java JNI exports plus `JNI_OnLoad` match
  the Android manifests, including the 13 archive V2 exports; retired parser/GIF
  exports are absent.
- APK native ABIs are ARM64-only. Both `libehviewer.so` and the retained transitive
  `libandroidx.graphics.path.so` are present; ARM64-only does not mean only one
  native library.
- Release runtime dependency report contains neither XMLutil serialization
  artifact nor the retired Kotlin diff library.
- All eight P3 and six P4 independent production coverage gates rerun and pass.
  Each report has exactly one expected production owner; no denominator borrowing.
- CI YAML parses successfully, retired-source checks pass, `git diff --check`
  passes. DAO source and every database schema/migration are unchanged.

Git submission follows separate operator approval; no remote push is part of this change.

Real Android device/SAF/UI execution and latency/peak-memory measurement are
not performed. This cleanup does not establish exhaustive runtime equivalence
or claim that every remaining no-caller graph node is dead.
