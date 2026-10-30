#!/usr/bin/env bash
# Rebuild the current host library before running; never load an older probe artifact.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root/app/src/main/rust"
probe=target/followup-jni
mkdir -p "$probe"
cc -shared -o "$probe/libehviewer-host.so" \
  -Wl,--whole-archive target/debug/libehviewer_rust.a \
  -Wl,--no-whole-archive -ldl -lpthread -lm
for fixture in RecoveryMapping DownloadFiles UpdatePackage CommentDisplay DownloadStartup CatalogState LabelRules EntryActions QuickSearchEdit ListOrder FavoriteBatch RecoveryPersistence ReaderNavigation ThumbnailFlow; do
  javac -d "$probe" "$root/scripts/fixtures/${fixture}JniSmoke.java"
done
for class in download.RecoveryMapping files.DownloadFiles updater.UpdatePackage gallery.CommentDisplay download.DownloadStartup gallery.CatalogState download.LabelRules download.EntryActions gallery.QuickSearchEdit library.ListOrder gallery.FavoriteBatch download.RecoveryPersistence reader.ReaderNavigation files.ThumbnailFlow; do
  java --enable-native-access=ALL-UNNAMED --add-opens=java.base/java.io=ALL-UNNAMED \
    -cp "$probe" "com.hippo.ehviewer.bridge.${class}BridgeKt" \
    "$probe/libehviewer-host.so" crates/domain/test_data
done
