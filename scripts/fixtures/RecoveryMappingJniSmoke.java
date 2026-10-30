// Host oracle for the exact production JNI signature; no Android or network effects.
package com.hippo.ehviewer.bridge.download;

import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.*;

class RecoveryMappingBridgeKt {
    private static native int nativePlanRecoveryMappingsV1(ByteBuffer request, int size, ByteBuffer response);
    private interface Probe { void run(); }
    private static void rejects(Probe action) {
        try { action.run(); } catch (RuntimeException expected) { return; }
        throw new AssertionError("Invalid transport accepted");
    }
    private static Map<String, Object> map(Object... fields) {
        var result = new LinkedHashMap<String, Object>();
        for (int i = 0; i < fields.length; i += 2) result.put((String) fields[i], fields[i + 1]);
        return result;
    }
    private static byte[] utf16(String value) {
        if (value == null) return null;
        var bytes = ByteBuffer.allocate(value.length() * 2).order(java.nio.ByteOrder.LITTLE_ENDIAN);
        for (char unit : value.toCharArray()) bytes.putChar(unit);
        return bytes.array();
    }
    private static void head(ByteArrayOutputStream out, int major, long value) {
        if (value >= 0 && value < 24) { out.write(major * 32 + (int) value); return; }
        int count = value >= 0 && value < 256 ? 1 : value >= 0 && value < 65536 ? 2 : value >= 0 && value <= 0xffffffffL ? 4 : 8;
        out.write(major * 32 + (count == 1 ? 24 : count == 2 ? 25 : count == 4 ? 26 : 27));
        for (int n = count - 1; n >= 0; n--) out.write((int) (value >>> (n * 8)) & 255);
    }
    private static void encode(ByteArrayOutputStream out, Object value) {
        if (value == null) { out.write(246); return; }
        if (value instanceof Number number) { long n = number.longValue(); head(out, n < 0 ? 1 : 0, n < 0 ? ~n : n); return; }
        if (value instanceof String text) { byte[] bytes = text.getBytes(StandardCharsets.UTF_8); head(out, 3, bytes.length); out.writeBytes(bytes); return; }
        if (value instanceof byte[] bytes) { head(out, 2, bytes.length); out.writeBytes(bytes); return; }
        if (value instanceof List<?> list) { head(out, 4, list.size()); for (Object item : list) encode(out, item); return; }
        if (value instanceof Map<?, ?> map) { head(out, 5, map.size()); for (var entry : map.entrySet()) { encode(out, entry.getKey()); encode(out, entry.getValue()); } return; }
        throw new AssertionError("Unsupported oracle value");
    }
    private static byte[] cbor(Object value) { var out = new ByteArrayOutputStream(); encode(out, value); return out.toByteArray(); }
    private static void check(byte[] request, byte[] expected) {
        ByteBuffer input = ByteBuffer.allocateDirect(request.length).put(request).flip();
        ByteBuffer output = ByteBuffer.allocateDirect(expected.length);
        int size = nativePlanRecoveryMappingsV1(input, request.length, output);
        byte[] actual = new byte[size]; output.get(actual);
        if (!Arrays.equals(expected, actual)) throw new AssertionError("Native association differs from Java oracle");
        byte[] unchanged = new byte[request.length]; input.get(unchanged);
        if (!Arrays.equals(request, unchanged)) throw new AssertionError("Input changed");
        var tiny = ByteBuffer.allocateDirect(1).put((byte) 0x42).flip();
        rejects(() -> nativePlanRecoveryMappingsV1(input, request.length, tiny));
        if (tiny.get(0) != 0x42) throw new AssertionError("Partial native output");
        for (int length : new int[] {-1, 0, request.length + 1, 32 * 1024 * 1024 + 1})
            rejects(() -> nativePlanRecoveryMappingsV1(input, length, output));
        rejects(() -> nativePlanRecoveryMappingsV1(ByteBuffer.wrap(request), request.length, output));
        rejects(() -> nativePlanRecoveryMappingsV1(input, request.length, ByteBuffer.allocate(1)));
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        check(HexFormat.of().parseHex(Files.readString(root.resolve("recovery_mapping_request.cbor.hex")).trim()),
              HexFormat.of().parseHex(Files.readString(root.resolve("recovery_mapping_response.cbor.hex")).trim()));
        String[] paths = {null, "", "x", "X", "😀", "\ud800", "\udc00", "?", "\ud800\ud800", "😀\ud800\udc01\udc00", "path:尾"};
        var candidates = new ArrayList<Map<String, Object>>();
        for (int i = 0; i < paths.length; i++) {
            candidates.add(map("gid", 7L, "status", "conflict", "sourceKey", "0".repeat(64), "pathUtf16", utf16(paths[i])));
            candidates.add(map("gid", 7L, "status", "ready", "sourceKey", String.format("%064x", i + 1), "pathUtf16", utf16(paths[i])));
            candidates.add(map("gid", 7L, "status", "ready", "sourceKey", "f".repeat(64), "pathUtf16", utf16(paths[i])));
        }
        var downloads = new ArrayList<Map<String, Object>>();
        var expected = new ArrayList<Map<String, Object>>();
        for (long gid : new long[] {7, 8, 0, Long.MIN_VALUE, Long.MAX_VALUE}) for (String path : paths) {
            byte[] raw = utf16(path);
            downloads.add(map("gid", gid, "dirnameUtf16", raw));
            Integer match = null;
            for (int i = 0; i < candidates.size(); i++) {
                var candidate = candidates.get(i);
                if (candidate.get("status").equals("ready") && candidate.get("gid").equals(gid) && Arrays.equals((byte[]) candidate.get("pathUtf16"), raw)) { match = i; break; }
            }
            String key = match == null ? HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(("database:" + gid + ":" + (path == null ? "" : path)).getBytes(StandardCharsets.UTF_8))) : (String) candidates.get(match).get("sourceKey");
            expected.add(map("candidateIndex", match, "sourceKey", key));
        }
        check(cbor(map("version", 1, "candidates", candidates, "downloads", downloads)),
              cbor(map("version", 1, "module", "recoveryMapping", "payload", map("mappings", expected), "error", null)));
        ByteBuffer malformed = ByteBuffer.allocateDirect(1).put((byte) 0xff).flip();
        rejects(() -> nativePlanRecoveryMappingsV1(malformed, 1, ByteBuffer.allocateDirect(100)));
        System.out.println("Recovery mappings host JNI: exact golden and 55 Java oracle rows; immutable input; atomic output; transport rejection");
    }
}
