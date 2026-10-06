// Host-only probe. Class name matches production Kotlin JNI facade exactly.
package com.hippo.ehviewer.bridge.archive;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class ComicInfoPolicyBridgeKt {
    private static native int nativePlanComicInfoPolicyV1(ByteBuffer request, int size, ByteBuffer response) throws Exception;
    private interface Probe { void run() throws Exception; }
    private static void failure(Probe probe) throws Exception {
        try { probe.run(); } catch (Exception expected) { return; }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for (String name : new String[] { "read", "create", "detailed", "rewrite", "keep", "read_export", "resolve_known", "resolve_metadata", "write", "skip", "invalid_pages", "missing_dir", "export_missing" }) {
            var root = Path.of(args[1]);
            byte[] request = HexFormat.of().parseHex(Files.readString(root.resolve("comic_info_policy_" + name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(Files.readString(root.resolve("comic_info_policy_" + name + "_response.cbor.hex")).trim());
            var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
            var output = ByteBuffer.allocateDirect(4096);
            int count = nativePlanComicInfoPolicyV1(input, request.length, output);
            if (count != expected.length) throw new AssertionError("Invalid JNI response size: " + name);
            byte[] actual = new byte[count]; output.get(actual);
            if (!Arrays.equals(actual, expected)) throw new AssertionError("JNI response differs from frozen Rust fixture: " + name);
            byte[] after = new byte[request.length]; input.get(after);
            if (!Arrays.equals(after, request)) throw new AssertionError("JNI mutated input");
            failure(() -> nativePlanComicInfoPolicyV1(input, -1, output));
            failure(() -> nativePlanComicInfoPolicyV1(input, 0, output));
            failure(() -> nativePlanComicInfoPolicyV1(input, request.length + 1, output));
            failure(() -> nativePlanComicInfoPolicyV1(input, 4097, output));
            failure(() -> nativePlanComicInfoPolicyV1(ByteBuffer.wrap(request), request.length, output));
            var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
            failure(() -> nativePlanComicInfoPolicyV1(input, request.length, tiny));
            if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Failed JNI response wrote partial output");
        }
        System.out.println("ComicInfo policy host JNI: 13 exact responses, immutable input, bounded requests, direct buffers, atomic output");
    }
}
