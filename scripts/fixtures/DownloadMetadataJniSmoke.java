// Host-only probe; class matches production Kotlin JNI facade exactly.
package com.hippo.ehviewer.bridge.download;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class DownloadMetadataBridgeKt {
    private static native int nativePlanDownloadMetadataV1(ByteBuffer request, int size, ByteBuffer response) throws Exception;
    private interface Probe { void run() throws Exception; }
    private static Exception failure(Probe probe) throws Exception {
        try { probe.run(); } catch (Exception expected) { return expected; }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for (String name : new String[] { "observe", "patch", "noop" }) {
            var root = Path.of(args[1]);
            byte[] request = HexFormat.of().parseHex(Files.readString(root.resolve("download_metadata_" + name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(Files.readString(root.resolve("download_metadata_" + name + "_response.cbor.hex")).trim());
            var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
            var output = ByteBuffer.allocateDirect(4096);
            int count = nativePlanDownloadMetadataV1(input, request.length, output);
            if (count != expected.length) throw new AssertionError("Invalid response size");
            byte[] actual = new byte[count]; output.get(actual);
            if (!Arrays.equals(actual, expected)) throw new AssertionError("Exact JNI golden differs");
            byte[] after = new byte[request.length]; input.get(after);
            if (!Arrays.equals(after, request)) throw new AssertionError("Input mutated");
            for (int size : new int[] { -1, 0, request.length + 1, 3 * 1024 * 1024 + 1 }) failure(() -> nativePlanDownloadMetadataV1(input, size, output));
            failure(() -> nativePlanDownloadMetadataV1(ByteBuffer.wrap(request), request.length, output));
            var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
            failure(() -> nativePlanDownloadMetadataV1(input, request.length, tiny));
            if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Partial output on failure");
            var malformed = ByteBuffer.allocateDirect(1).put((byte) 0xff).flip();
            if (!failure(() -> nativePlanDownloadMetadataV1(malformed, 1, output)).getMessage().contains("Invalid download metadata CBOR")) throw new AssertionError("Non-fixed malformed error");
            if (name.equals("observe")) {
                byte[] invalidEnum = request.clone();
                byte[] needle = "observe".getBytes(StandardCharsets.UTF_8);
                int offset = -1;
                for (int i = 0; i <= invalidEnum.length - needle.length; i++) {
                    if (Arrays.equals(Arrays.copyOfRange(invalidEnum, i, i + needle.length), needle)) { offset = i; break; }
                }
                if (offset < 0) throw new AssertionError("Enum fixture missing");
                System.arraycopy("private".getBytes(StandardCharsets.UTF_8), 0, invalidEnum, offset, needle.length);
                var invalid = ByteBuffer.allocateDirect(invalidEnum.length).put(invalidEnum).flip();
                var error = failure(() -> nativePlanDownloadMetadataV1(invalid, invalidEnum.length, output));
                if (!error.getMessage().contains("Invalid download metadata CBOR") || error.getMessage().contains("private")) throw new AssertionError("Enum error leaked payload");
            }
        }
        System.out.println("Download metadata host JNI: 3 exact envelopes; immutable input; size/direct-buffer/atomic-output/malformed/enum/redaction probes passed");
    }
}
