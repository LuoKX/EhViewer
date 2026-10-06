// Host-only stateful probe. No Android classes, paths or credentials enter native scan state.
package com.hippo.ehviewer.bridge.download;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HexFormat;

class RecoveryScanBridgeKt {
    private static native long nativeOpenRecoveryScanV1(ByteBuffer request, int size);
    private static native long nativeStepRecoveryScanV1(long handle, int operation, long argument);
    private static native long nativeObserveRecoveryDocumentV1(long handle, ByteBuffer identity, int size);
    private interface Probe { void run(); }
    private static void failure(Probe probe) {
        try { probe.run(); } catch (RuntimeException expected) { return; }
        throw new AssertionError("Native failure hidden");
    }
    private static void same(long expected, long actual) {
        if (expected != actual) throw new AssertionError("Expected " + expected + ", got " + actual);
    }
    private static long ticket(int index, int depth) { return ((long) index << 32) | depth; }
    private static ByteBuffer identity(String value) {
        var buffer = ByteBuffer.allocateDirect(Math.max(2, value.length() * 2)).order(ByteOrder.LITTLE_ENDIAN);
        for (int i = 0; i < value.length(); i++) buffer.putChar(value.charAt(i));
        return buffer.flip();
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        byte[] physicalBytes = HexFormat.of().parseHex(Files.readString(root.resolve("recovery_scan_physical_open.cbor.hex")).trim());
        byte[] documentBytes = HexFormat.of().parseHex(Files.readString(root.resolve("recovery_scan_documents_open.cbor.hex")).trim());
        var physical = ByteBuffer.allocateDirect(physicalBytes.length).put(physicalBytes).flip();
        var documents = ByteBuffer.allocateDirect(documentBytes.length).put(documentBytes).flip();
        long handle = nativeOpenRecoveryScanV1(physical, physicalBytes.length);
        if (handle <= 0) throw new AssertionError("No owned handle");
        try {
            same(0, nativeStepRecoveryScanV1(handle, 1, 0));
            failure(() -> nativeStepRecoveryScanV1(handle, 1, 0));
            same(1, nativeStepRecoveryScanV1(handle, 2, 0));
            same(ticket(1, 1), nativeStepRecoveryScanV1(handle, 3, 1));
            same(1, nativeStepRecoveryScanV1(handle, 2, 0));
            same(-1, nativeStepRecoveryScanV1(handle, 3, 2)); // physical symlink skipped
            same(1, nativeStepRecoveryScanV1(handle, 2, 0));
            same(ticket(2, 1), nativeStepRecoveryScanV1(handle, 3, 1));
            same(0, nativeStepRecoveryScanV1(handle, 4, 0));
            same(ticket(1, 1), nativeStepRecoveryScanV1(handle, 1, 0));
            same(1, nativeStepRecoveryScanV1(handle, 2, 0));
            same(-1, nativeStepRecoveryScanV1(handle, 3, 1)); // over-depth directory
            same(1, nativeStepRecoveryScanV1(handle, 5, 0));
            same(1, nativeStepRecoveryScanV1(handle, 2, 0));
            same(ticket(3, 2), nativeStepRecoveryScanV1(handle, 3, 0)); // loose file one level deeper
            same(0, nativeStepRecoveryScanV1(handle, 4, 0));
            same(-1, nativeStepRecoveryScanV1(handle, 1, 0)); // node cap with pending sibling
            same(1, nativeStepRecoveryScanV1(handle, 5, 0));
            failure(() -> nativeStepRecoveryScanV1(handle, 3, 4));
            failure(() -> nativeStepRecoveryScanV1(handle, 99, 0));
            failure(() -> nativeStepRecoveryScanV1(handle, 6, 1));
        } finally { nativeStepRecoveryScanV1(handle, 6, 0); }
        failure(() -> nativeStepRecoveryScanV1(handle, 5, 0));
        failure(() -> nativeStepRecoveryScanV1(handle, 6, 0));
        long documentHandle = nativeOpenRecoveryScanV1(documents, documentBytes.length);
        if (documentHandle <= handle) throw new AssertionError("Reused handle");
        try {
            same(0, nativeStepRecoveryScanV1(documentHandle, 1, 0));
            same(1, nativeStepRecoveryScanV1(documentHandle, 2, 0));
            var rootId = identity("r\uD800");
            same(0, nativeObserveRecoveryDocumentV1(documentHandle, rootId, rootId.remaining()));
            same(1, nativeStepRecoveryScanV1(documentHandle, 2, 0));
            var child = identity("a\uDC00");
            failure(() -> nativeObserveRecoveryDocumentV1(documentHandle, child, 1));
            failure(() -> nativeObserveRecoveryDocumentV1(documentHandle, ByteBuffer.allocate(2), 2));
            failure(() -> nativeObserveRecoveryDocumentV1(documentHandle, child, child.capacity() + 2));
            failure(() -> nativeObserveRecoveryDocumentV1(documentHandle, child, -1));
            failure(() -> nativeObserveRecoveryDocumentV1(documentHandle, child, 262_145));
            same(1, nativeObserveRecoveryDocumentV1(documentHandle, child, child.remaining()));
            failure(() -> nativeStepRecoveryScanV1(documentHandle, 3, 2));
            same(ticket(1, 1), nativeStepRecoveryScanV1(documentHandle, 3, 1));
            same(1, nativeStepRecoveryScanV1(documentHandle, 2, 0));
            same(0, nativeObserveRecoveryDocumentV1(documentHandle, child, child.remaining()));
            same(0, nativeStepRecoveryScanV1(documentHandle, 4, 0));
            same(ticket(1, 1), nativeStepRecoveryScanV1(documentHandle, 1, 0));
            same(0, nativeStepRecoveryScanV1(documentHandle, 4, 0));
            same(-1, nativeStepRecoveryScanV1(documentHandle, 1, 1_000_000));
            same(0, nativeStepRecoveryScanV1(documentHandle, 5, 0));
        } finally { nativeStepRecoveryScanV1(documentHandle, 6, 0); }
        failure(() -> nativeObserveRecoveryDocumentV1(documentHandle, identity("id"), 4));
        failure(() -> nativeOpenRecoveryScanV1(physical, 0));
        failure(() -> nativeOpenRecoveryScanV1(physical, -1));
        failure(() -> nativeOpenRecoveryScanV1(physical, physicalBytes.length + 1));
        failure(() -> nativeOpenRecoveryScanV1(physical, 262_145));
        failure(() -> nativeOpenRecoveryScanV1(ByteBuffer.wrap(physicalBytes), physicalBytes.length));
        long[] live = new long[4];
        try {
            for (int i = 0; i < live.length; i++) live[i] = nativeOpenRecoveryScanV1(physical, physicalBytes.length);
            failure(() -> nativeOpenRecoveryScanV1(physical, physicalBytes.length));
        } finally { for (long scan : live) if (scan != 0) nativeStepRecoveryScanV1(scan, 6, 0); }
        for (int i = 0; i < 500; i++) {
            long scan = nativeOpenRecoveryScanV1(physical, physicalBytes.length);
            same(0, nativeStepRecoveryScanV1(scan, 6, 0));
        }
        System.out.println("Recovery scan host JNI: BFS/depth/quota, raw UTF-16 dedup, phase errors, 4-session cap, 500 scoped closes, stale handles");
    }
}
