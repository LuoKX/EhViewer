// Host-only probe. Class name matches production Kotlin JNI facade.
package com.hippo.ehviewer.bridge.updater;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class UpdaterBridgeKt {
    private static native int nativePlanUpdateV1(ByteBuffer request, int size, ByteBuffer response);
    private interface Probe { void run(); }
    private static void failure(Probe probe) {
        try { probe.run(); } catch (RuntimeException expected) { return; }
        throw new AssertionError("Native failure hidden");
    }
    private static byte[] call(byte[] bytes) {
        var input = ByteBuffer.allocateDirect(bytes.length).put(bytes).flip();
        var output = ByteBuffer.allocateDirect(3 * 1024 * 1024);
        int size = nativePlanUpdateV1(input, bytes.length, output);
        var result = new byte[size]; output.get(result);
        var after = new byte[bytes.length]; input.get(after);
        if (!Arrays.equals(bytes, after)) throw new AssertionError("JNI mutated input");
        return result;
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        int tested = 0;
        try (var files = Files.list(root)) {
            for (var path : files.filter(p -> p.toString().endsWith("_request.cbor.hex")).sorted().toList()) {
                var bytes = HexFormat.of().parseHex(Files.readString(path).trim());
                var expected = HexFormat.of().parseHex(Files.readString(Path.of(path.toString().replace("_request", "_response"))).trim());
                if (!Arrays.equals(call(bytes), expected)) throw new AssertionError("JNI golden differs: " + path.getFileName());
                var input = ByteBuffer.allocateDirect(bytes.length).put(bytes).flip();
                var output = ByteBuffer.allocateDirect(4096);
                failure(() -> nativePlanUpdateV1(input, -1, output));
                failure(() -> nativePlanUpdateV1(input, 0, output));
                failure(() -> nativePlanUpdateV1(input, bytes.length + 1, output));
                failure(() -> nativePlanUpdateV1(input, 3 * 1024 * 1024 + 1, output));
                failure(() -> nativePlanUpdateV1(ByteBuffer.wrap(bytes), bytes.length, output));
                var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
                failure(() -> nativePlanUpdateV1(input, bytes.length, tiny));
                if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Partial JNI write");
                tested++;
            }
        }
        var fixedError = HexFormat.of().parseHex(Files.readString(root.resolve("unsupported_response.cbor.hex")).trim());
        if (!Arrays.equals(call(new byte[] {(byte) 0xff}), fixedError)) throw new AssertionError("Malformed CBOR error leaked");
        var invalidEnum = HexFormat.of().parseHex(Files.readString(root.resolve("skip_request.cbor.hex")).trim());
        var text = new String(invalidEnum, java.nio.charset.StandardCharsets.ISO_8859_1);
        int offset = text.indexOf("policy");
        System.arraycopy("secret".getBytes(java.nio.charset.StandardCharsets.US_ASCII), 0, invalidEnum, offset, 6);
        if (!Arrays.equals(call(invalidEnum), fixedError)) throw new AssertionError("Unknown enum error leaked");
        if (tested != 27) throw new AssertionError("Missing goldens");
        System.out.println("Updater host JNI: 27 exact envelopes, immutable input, malformed/enum redaction, bounded direct buffers, atomic output");
    }
}
