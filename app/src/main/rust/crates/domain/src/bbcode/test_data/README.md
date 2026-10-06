# Frozen edit oracle

`legacy-diff.tsv` contains 2,704 cases from the pinned production dependency
`io.github.petertrr:kotlin-multiplatform-diff-jvm:1.3.0`, not a Rust-generated
expectation. Generator: `scripts/fixtures/BBCodeDiffOracle.java`.

Maven artifacts, SHA-256:

- JVM jar: `db3d8270f30c0e92a03bd7f7ce99c17ed6dddf197df5b3c4154ad45facf698a8`
- Sources jar: `827417ea753f65260c5c2c3c06107774d780e14672e54d5142eca308b02324e6`

Compile the generator with that JVM jar and `kotlin-stdlib:2.4.0` on `-cp`, then
run `BBCodeDiffOracle` with the same classpath. Standard output is the TSV.
There is no oracle dependency in production, Rust tests, or Gradle tests.

Columns: source UTF-16 hex, target UTF-16 hex, delta count; for one delta only,
type, source start/end, target start/end. Each code unit is four hex digits;
empty text is an empty field. Offsets are Java UTF-16, not Unicode scalar indexes.
The corpus includes repetition/tie-breaking, multi-hunk edits, replacement,
CR/LF, CJK, emoji, NUL, and isolated high/low surrogates.

One CHANGE delta used to crash `updateSpan` through `unreachable()`. Rust instead
returns TARGET for this case, as well as existing multi-delta cases. All single
INSERT/DELETE positions match the old implementation exactly.

Myers path/tie-breaking code was adapted from the pinned Apache-2.0 source,
including original copyright notices. License: `../LICENSE-APACHE`.
