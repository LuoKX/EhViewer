# Original UI contract fixtures

Source-based characterization, **not renderer/device parity acceptance**.
Current slice freezes `SignInScreen.kt`, relevant resource qualifiers, labels,
vector/license and theme references. Canonical source inventory carries byte sizes
and SHA-256 hashes. Unknown guarded Kotlin changes fail closed; review facts before
updating supported hashes. Resource keys such as `app_waring` retain original spelling.

```sh
python3 -B native/parity/tools/export_signin_baseline.py --repo . --output native/parity/fixtures/signin --check
python3 -B -m unittest discover -s native/parity/tests
```

`--check`/`--verify` never rewrites fixtures and exits nonzero on drift/missing
artifacts. Without check, generation is intentional and must follow source review.
Live Git HEAD is printed separately, never embedded into deterministic fixtures:
committing unrelated changes must not invalidate source-bound contracts.
Historical reference revision is not a claim that current tree is clean.

Current baseline includes:
- compact/medium versus expanded order, different primary-button ordering,
  resource widths/margins, modifier ordering and inline dp facts;
- 12 locales / 13 keys, explicit missing/conflicting declarations and fallback
  candidates; raw XML text/escapes/format markers are preserved;
- original vector and GPL-3.0-or-later notice/license bytes;
- username-first exact-emptiness validation, sticky errors, password reveal mode,
  failure-dialog resources and cancellation behavior;
- theme defaults, dynamic/wallpaper/fallback branches and expressive motion references.

Numeric AndroidX expanded breakpoint remains symbolic until dependency definition
is resolved with provenance. Android qualifier/locale resolution, Material3
typography/shapes/ripple/motion, dynamic colors, IME/autofill/accessibility/insets,
password reveal timing, rasterization and screenshots remain separate gates.
Do not guess 840dp or treat matching labels as matching UI.

Exporter accepts bounded repository XML and rejects DTD/entity declarations.
Tests mutate only temporary copied source fixtures, never production data.
