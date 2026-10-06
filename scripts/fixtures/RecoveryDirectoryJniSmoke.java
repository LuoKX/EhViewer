// Host-only transport probe. Production Kotlin file-class and signature must match.
package com.hippo.ehviewer.bridge.download;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class RecoveryDirectoryBridgeKt {
    private static native int nativePlanRecoveryDirectoryV1(ByteBuffer request, int size, ByteBuffer response);
    private interface Probe { void run(); }
    private static void failure(String message, Probe probe) {
        try { probe.run(); } catch (RuntimeException expected) {
            if (message != null && !message.equals(expected.getMessage())) throw new AssertionError("Unsanitized error", expected);
            return;
        }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        for (String name : new String[] { "observe", "promote" }) {
            byte[] request = HexFormat.of().parseHex(Files.readString(root.resolve("recovery_directory_" + name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(Files.readString(root.resolve("recovery_directory_" + name + "_response.cbor.hex")).trim());
            var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
            var output = ByteBuffer.allocateDirect(4096);
            int count = nativePlanRecoveryDirectoryV1(input, request.length, output);
            byte[] actual = new byte[count]; output.get(actual);
            if (!Arrays.equals(actual, expected)) throw new AssertionError("Golden response differs: " + name);
            byte[] after = new byte[request.length]; input.duplicate().get(after);
            if (!Arrays.equals(after, request)) throw new AssertionError("Native mutated request");
            failure("Invalid recovery directory request size", () -> nativePlanRecoveryDirectoryV1(input, -1, output));
            failure("Invalid recovery directory request size", () -> nativePlanRecoveryDirectoryV1(input, 0, output));
            failure(null, () -> nativePlanRecoveryDirectoryV1(input, request.length + 1, output));
            failure("Invalid recovery directory request size", () -> nativePlanRecoveryDirectoryV1(input, 33_554_433, output));
            failure(null, () -> nativePlanRecoveryDirectoryV1(ByteBuffer.wrap(request), request.length, output));
            failure(null, () -> nativePlanRecoveryDirectoryV1(input, request.length, ByteBuffer.allocate(4096)));
            var malformed = ByteBuffer.allocateDirect(1).put((byte) 0xff).flip();
            failure("Invalid recovery directory CBOR request", () -> nativePlanRecoveryDirectoryV1(malformed, 1, output));
            var unknown = request.clone();
            byte[] marker = name.getBytes(java.nio.charset.StandardCharsets.UTF_8);
            for (int i = 0; i + marker.length <= unknown.length; i++) {
                if (Arrays.equals(Arrays.copyOfRange(unknown, i, i + marker.length), marker)) { Arrays.fill(unknown, i, i + marker.length, (byte) 'x'); break; }
            }
            var badEnum = ByteBuffer.allocateDirect(unknown.length).put(unknown).flip();
            failure("Invalid recovery directory CBOR request", () -> nativePlanRecoveryDirectoryV1(badEnum, unknown.length, output));
            var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
            failure(null, () -> nativePlanRecoveryDirectoryV1(input, request.length, tiny));
            if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Partial native output");
        }
        // Characterize platform assumptions explicitly, independent of Rust implementation.
        for (int code = 0; code <= 0xffff; code++) {
            char unit = (char) code;
            boolean whitespace = Character.isWhitespace(unit) || Character.isSpaceChar(unit);
            boolean frozen = (code >= 9 && code <= 13) || (code >= 28 && code <= 32) || code == 0xa0 || code == 0x1680 ||
                (code >= 0x2000 && code <= 0x200a) || code == 0x2028 || code == 0x2029 || code == 0x202f || code == 0x205f || code == 0x3000;
            if (whitespace != frozen) throw new AssertionError("JVM whitespace contract differs: " + code);
            for (char ascii : ".ehviewercomicinfoxmlcbzzip0123456789".toCharArray()) {
                boolean matches = String.valueOf(unit).equalsIgnoreCase(String.valueOf(ascii));
                boolean expected = Character.toLowerCase(unit) == ascii && code < 128 || ascii == 'i' && (code == 0x130 || code == 0x131);
                if (matches != expected) throw new AssertionError("JVM case contract differs: " + code);
            }
        }
        System.out.println("Recovery directory host JNI: 2 exact responses; immutable inputs; bounded/malformed/enum/direct-buffer errors; atomic output; exhaustive 65536-unit JVM case/blank characterization");
    }
}
