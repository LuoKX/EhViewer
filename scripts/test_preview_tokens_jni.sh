#!/usr/bin/env bash
# Run after cargo build --features jvm; use this checkout's artifacts only.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root/app/src/main/rust"
probe=target/preview-tokens-jni
mkdir -p "$probe"
cc -shared -o "$probe/libehviewer-host.so" \
  -Wl,--whole-archive target/debug/libehviewer_rust.a \
  -Wl,--no-whole-archive -ldl -lpthread -lm
javac -d "$probe" "$root/scripts/fixtures/PreviewTokensJniSmoke.java"
java --enable-native-access=ALL-UNNAMED -cp "$probe" \
  com.hippo.ehviewer.bridge.reader.PreviewTokenBridgeKt \
  "$probe/libehviewer-host.so" crates/domain/test_data
