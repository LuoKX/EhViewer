// Host-only probe; production Kotlin file-class name must match exactly.
package com.hippo.ehviewer.bridge.files;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class StorageRulesBridgeKt {
    private static native int nativePlanStorageRulesV1(ByteBuffer request, int size, ByteBuffer response);
    private interface Probe { void run(); }
    private static void failure(Probe probe) {
        try { probe.run(); } catch (RuntimeException expected) { return; }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        for (String name : new String[] { "classify", "candidates", "names", "cleanup" }) {
            byte[] request = HexFormat.of().parseHex(Files.readString(root.resolve("storage_" + name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(Files.readString(root.resolve("storage_" + name + "_response.cbor.hex")).trim());
            var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
            var output = ByteBuffer.allocateDirect(4096);
            int count = nativePlanStorageRulesV1(input, request.length, output);
            if (count != expected.length) throw new AssertionError("Wrong response length: " + name);
            byte[] actual = new byte[count]; output.get(actual);
            if (!Arrays.equals(actual, expected)) throw new AssertionError("Golden response differs: " + name);
            byte[] after = new byte[request.length]; input.duplicate().get(after);
            if (!Arrays.equals(after, request)) throw new AssertionError("Native mutated request");
            failure(() -> nativePlanStorageRulesV1(input, -1, output));
            failure(() -> nativePlanStorageRulesV1(input, 0, output));
            failure(() -> nativePlanStorageRulesV1(input, request.length + 1, output));
            failure(() -> nativePlanStorageRulesV1(input, 33_554_433, output));
            failure(() -> nativePlanStorageRulesV1(ByteBuffer.wrap(request), request.length, output));
            failure(() -> nativePlanStorageRulesV1(input, request.length, ByteBuffer.allocate(4096)));
            var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
            failure(() -> nativePlanStorageRulesV1(input, request.length, tiny));
            if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Partial native output");
        }
        System.out.println("Storage host JNI: 4 exact responses, immutable input, bounds/direct buffers, atomic output");
    }
}
