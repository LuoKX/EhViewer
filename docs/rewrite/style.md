# Code Style and Naming

## Principles

Google's readability priorities—clarity, simplicity, concision,
maintainability, and consistency—apply across languages. Automated tools own
layout; review focuses on names, boundaries, invariants, and behavior.

## Kotlin and Gradle Kotlin DSL

- Format with Spotless and ktfmt Google Style.
- Types use `UpperCamelCase`; functions and properties use `lowerCamelCase`;
  constants use `UPPER_SNAKE_CASE`.
- Treat acronyms as words: `HttpClient`, `parseHttpUrl`, and `cborPayload`.
- Package names are lowercase domain nouns.
- Public bridge types and native methods require KDoc that identifies their
  protocol version and Rust owner.

During migration, Spotless uses `origin/main` as a ratchet: every new or touched
Kotlin/Kotlin Gradle file must use ktfmt Google Style, while untouched legacy
files are reformatted with their feature slice. The release-candidate milestone
removes the ratchet only after the remaining legacy tree has been formatted or
deleted; no grandfathered style is allowed in the completed rewrite.

Reference: [Google Kotlin Style Guide](https://developer.android.com/kotlin/style-guide).

## Rust

- `rustfmt` is the formatting authority.
- Types and traits use `UpperCamelCase`; functions, modules, and fields use
  `snake_case`; constants use `SCREAMING_SNAKE_CASE`.
- Crates use the `eh_<domain>` pattern; directories use the short domain name.
- Public items require rustdoc with semantics, errors, examples where useful,
  and safety details where applicable.
- Every `unsafe` block has a precise `// SAFETY:` invariant.
- Avoid non-domain abbreviations and single-letter names outside tiny scopes.

Rust has no separate public Google formatter profile. This project therefore
uses canonical `rustfmt` layout and Rust API naming while applying Google's
readability principles to API design and review.

## Automated Naming Gate

CI runs `python3 scripts/check_rewrite_names.py` in addition to rustfmt, Clippy,
and Spotless. The ratcheted checker validates `eh_<domain>` crate/package alignment,
snake-case Rust source/module paths, lowercase Kotlin bridge packages,
UpperCamelCase bridge files/types, lowerCamelCase functions/properties, and
UPPER_SNAKE_CASE constants. It rejects new `common`, `helpers`, `manager`, `misc`,
and `utils` catch-all modules. Positive and intentionally invalid repository
fixtures test the checker itself before it audits the real tree.

## Comments

Good comments explain a constraint or rationale:

```rust
// Keep the discriminator stable because Kotlin golden fixtures persist it.
```

Do not narrate an obvious statement:

```rust
// Increment the page count.
page_count += 1;
```

TODOs include an issue/task reference and a removal condition.
