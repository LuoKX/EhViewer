// Host-only class name and native signature match production Kotlin exactly.
package com.hippo.ehviewer.bridge.reader;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class PreviewTokenBridgeKt {
    private static native byte[] nativePlanPreviewTokensV1(
        ByteBuffer request, int size) throws Exception;

    private interface Probe { void run() throws Exception; }
    private static void failure(Probe probe) throws Exception {
        try { probe.run(); } catch (Exception expected) {
            if (expected.getMessage().contains("secret-token")) {
                throw new AssertionError("Sensitive payload reflected in error");
            }
            return;
        }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        Path root = Path.of(args[1]);
        for (String name : new String[] {
            "lookup", "fallback", "apply", "empty_first", "empty_nonzero",
            "overflow", "version"
        }) {
            byte[] request = HexFormat.of().parseHex(Files.readString(
                root.resolve("preview_tokens_" + name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(Files.readString(
                root.resolve("preview_tokens_" + name + "_response.cbor.hex")).trim());
            ByteBuffer input = ByteBuffer.allocateDirect(request.length)
                .put(request).flip();
            byte[] actual = nativePlanPreviewTokensV1(input, request.length);
            if (!Arrays.equals(actual, expected)) {
                throw new AssertionError("JNI golden mismatch: " + name);
            }
            byte[] after = new byte[request.length]; input.get(after);
            if (!Arrays.equals(after, request)) {
                throw new AssertionError("Input mutated");
            }
            failure(() -> nativePlanPreviewTokensV1(input, -1));
            failure(() -> nativePlanPreviewTokensV1(input, 0));
            failure(() -> nativePlanPreviewTokensV1(input, request.length + 1));
            failure(() -> nativePlanPreviewTokensV1(input, 2 * 1024 * 1024 + 1));
            failure(() -> nativePlanPreviewTokensV1(ByteBuffer.wrap(request),
                request.length));
        }
        byte[] valid = HexFormat.of().parseHex(Files.readString(
            root.resolve("preview_tokens_lookup_request.cbor.hex")).trim());
        String hex = HexFormat.of().formatHex(valid);
        // Closed enum error must not expose the rejected value from serde.
        byte[] unknown = HexFormat.of().parseHex(hex.replace(
            "666c6f6f6b7570", "6c7365637265742d746f6b656e"));
        failure(() -> nativePlanPreviewTokensV1(
            ByteBuffer.allocateDirect(unknown.length).put(unknown).flip(),
            unknown.length));
        byte[] malformed = new byte[] {(byte) 0xff};
        failure(() -> nativePlanPreviewTokensV1(
            ByteBuffer.allocateDirect(1).put(malformed).flip(), 1));
        byte[] trailing = Arrays.copyOf(valid, valid.length + 1);
        failure(() -> nativePlanPreviewTokensV1(
            ByteBuffer.allocateDirect(trailing.length).put(trailing).flip(),
            trailing.length));
        System.out.println("Preview tokens host JNI: 7 exact envelopes; "
            + "immutable input; size/direct-buffer/enum/CBOR rejection; "
            + "redacted errors");
    }
}
