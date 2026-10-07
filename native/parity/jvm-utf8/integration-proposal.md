# Parent-only integration proposal — NOT applied

Target parent preferences owner; current `preferences.rs` remains byte-identical to supplied candidate. Seam registration only adds `pub mod jvm_utf8;` in isolated lane's `lib.rs`. Parent may omit registration until approved integration.

Exact proposed changes, after parent review:

1. Keep `Cursor::string` unchanged. `Parser::entry` tag 10 stays `key = cursor.string()?`; malformed map keys remain `InvalidUtf8`.
2. Add only this sibling Cursor helper:

```rust
fn replacement_string(&mut self) -> Result<String, Error> {
    crate::jvm_utf8::decode(self.bytes()?, MAX_BYTES)
        .map(crate::jvm_utf8::Decoded::into_string)
        .map_err(|_| Error::LimitExceeded)
}
```

3. Change only `Parser::value` tag 42 to `Value::String(cursor.replacement_string()?)` and `Parser::set` tag 10 to `set.insert(cursor.replacement_string()?)?;`.

Helper consumes complete length-delimited byte body or returns redacted failure. Both helper limit/allocation failure map to existing conservative `LimitExceeded`; no new preference error/fallback is required. Parent must review that error mapping before applying.

Do not change `Parser::skip`, unknown-field marker, retained original document, read-only mutation guard, duplicate-oneof/map resolution, unset-value reduction, string-set repeated-item count, or `PreferenceSet::insert`. Dedup occurs AFTER replacement: distinct malformed byte bodies may now yield same string; first decoded encounter wins. Generated protobuf itself retains repeated strings (oracle confirms duplicate count two); ordered-set dedup belongs to existing serializer/Rust policy, not decoder.

Keep `MAX_BYTES`, `MAX_FIELDS`, `MAX_ENTRIES`, `MAX_SET_STRINGS`, group depth, set encoded-byte accounting, and encode/mutation validation unchanged. Pass existing `MAX_BYTES` as per-string canonical-output ceiling. Decoder ceiling is not a new document aggregate budget: input wire stays one MiB; replacement can grow canonical text up to three times source bytes across document. Existing set/encode/mutation checks still apply. Parent must decide separately if decode-time aggregate canonical validation is required; do not silently introduce one in this seam.

Parent integration tests required (old identities remain immutable): accept existing invalid-value and invalid-set fixtures with JVM replacement; still reject invalid-key; replay surrogate/prefix quirks; dedup equivalent replacement strings in first-occurrence order; prove unknown-wire retention/read-only even with malformed value/set; test replacement output and existing item/field/set/document budget edges. Existing strict-subset test expectations/documentation need parent-owned update, not golden refresh.

Sharp gate: host OpenJDK 25.0.4.1 synthetic equality does NOT establish Android libcore/device equality. No Android sources/runtime/device proof available here. Android metadata compile proves types only. Parent must obtain approved pinned Android representation/consumption evidence before product/device/cutover acceptance. No generic `from_utf8_lossy`, Kotlin fallback, or integration applied.
