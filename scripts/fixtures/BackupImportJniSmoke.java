// Host-only real JNI boundary probe; Kotlin file-class name and shape match production.
package com.hippo.ehviewer.bridge.download;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class BackupImportBridgeKt {
    private static native int nativePlanBackupImportV1(ByteBuffer request, int size, ByteBuffer response);
    private interface Probe { void run(); }
    private static void failure(Probe probe) {
        try { probe.run(); } catch (RuntimeException expected) {
            if (expected.getMessage().contains("private-backup-name")) throw new AssertionError("Payload leaked");
            return;
        }
        throw new AssertionError("Native failure hidden");
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        byte[] request = HexFormat.of().parseHex(Files.readString(root.resolve("backup_import_request.cbor.hex")).trim());
        byte[] expected = HexFormat.of().parseHex(Files.readString(root.resolve("backup_import_response.cbor.hex")).trim());
        var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
        var output = ByteBuffer.allocateDirect(4096);
        int count = nativePlanBackupImportV1(input, request.length, output);
        byte[] actual = new byte[count]; output.get(actual);
        if (!Arrays.equals(actual, expected)) throw new AssertionError("Golden response differs");
        byte[] after = new byte[request.length]; input.duplicate().get(after);
        if (!Arrays.equals(after, request)) throw new AssertionError("Native mutated request");
        failure(() -> nativePlanBackupImportV1(input, -1, output));
        failure(() -> nativePlanBackupImportV1(input, 0, output));
        failure(() -> nativePlanBackupImportV1(input, request.length + 1, output));
        failure(() -> nativePlanBackupImportV1(input, 8_388_609, output));
        failure(() -> nativePlanBackupImportV1(ByteBuffer.wrap(request), request.length, output));
        failure(() -> nativePlanBackupImportV1(input, request.length, ByteBuffer.allocate(4096)));
        var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x5a).flip();
        failure(() -> nativePlanBackupImportV1(input, request.length, tiny));
        if (tiny.get(0) != (byte) 0x5a) throw new AssertionError("Partial native output");
        for (byte[] bad : new byte[][] { {(byte) 0xff}, HexFormat.of().parseHex("a173707269766174652d6261636b75702d6e616d65f6") }) {
            var malformed = ByteBuffer.allocateDirect(bad.length).put(bad).flip();
            failure(() -> nativePlanBackupImportV1(malformed, bad.length, output));
        }
        System.out.println("Backup import host JNI: exact combined golden, immutable input, malformed/redacted input, size/direct-buffer bounds, atomic output");
    }
}
