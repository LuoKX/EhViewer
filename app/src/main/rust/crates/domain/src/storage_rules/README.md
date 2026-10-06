# Legacy storage ordering reference

`legacy_oracle.jsonl` freezes 1,536 deterministic JDK reference cases: 512 each
for Java String name ordering, stable newest-first file retention and legacy
recovery candidate ordering/capping. Inputs include duplicate names, raw lone
surrogates, supplementary characters, signed extreme timestamps, empty arrays,
zero/negative retention counts and directory/archive identity ties.

Reference generator reproduces retired Kotlin operations via their JDK
comparators: directories first then archives, stable `sourceId` sorting, and
stable descending modification timestamps followed by `index >= maxFiles`.
It uses only generated observations, no production paths or user data. It is
never loaded by Android or normal tests.

Regenerate/verify from repository root (JDK 17+):

```sh
mkdir -p /tmp/eh-storage-oracle
javac -d /tmp/eh-storage-oracle scripts/fixtures/StorageRulesDiffOracle.java
java -cp /tmp/eh-storage-oracle StorageRulesDiffOracle \
  | cmp app/src/main/rust/crates/domain/src/storage_rules/legacy_oracle.jsonl -
```

Rust consumes frozen results under `storage_rules::tests::legacy`.
Classification and malformed/bounded CBOR tests are separate. Device page-name
Regex observations are deliberately not replaced with a host JDK or Rust
Unicode case-folding implementation.
