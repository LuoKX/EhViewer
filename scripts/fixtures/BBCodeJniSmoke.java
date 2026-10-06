// Host-only probe. Class name deliberately matches production Kotlin JNI facade.
package com.hippo.ehviewer.bridge.text;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class BBCodeBridgeKt {
    private static native int nativePlanBBCodeV1(ByteBuffer request, int size, ByteBuffer response) throws Exception;
    private interface Probe { void run() throws Exception; }
    private static void failure(Probe probe) throws Exception {
        try { probe.run(); } catch (Exception expected) { return; }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for (String name : new String[] { "bbcode_export", "bbcode_edit" }) {
            var root = Path.of(args[1]);
            byte[] request = HexFormat.of().parseHex(Files.readString(root.resolve(name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(Files.readString(root.resolve(name + "_response.cbor.hex")).trim());
            var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
            var output = ByteBuffer.allocateDirect(4096);
            int count = nativePlanBBCodeV1(input, request.length, output);
            if (count != expected.length) throw new AssertionError("Invalid JNI response size");
            byte[] actual = new byte[count]; output.get(actual);
            if (!Arrays.equals(actual, expected)) throw new AssertionError("JNI response differs from frozen Rust fixture");
            byte[] after = new byte[request.length]; input.get(after);
            if (!Arrays.equals(after, request)) throw new AssertionError("JNI mutated input");
            failure(() -> nativePlanBBCodeV1(input, -1, output));
            failure(() -> nativePlanBBCodeV1(input, 0, output));
            failure(() -> nativePlanBBCodeV1(input, request.length + 1, output));
            failure(() -> nativePlanBBCodeV1(input, 8 * 1024 * 1024 + 1, output));
            failure(() -> nativePlanBBCodeV1(ByteBuffer.wrap(request), request.length, output));
            var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
            failure(() -> nativePlanBBCodeV1(input, request.length, tiny));
            if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Failed JNI response wrote partial output");
        }
        System.out.println("BBCode host JNI: 2 exact responses, immutable input, bounded requests, direct buffers, atomic output");
    }
}
