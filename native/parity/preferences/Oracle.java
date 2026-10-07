import androidx.datastore.preferences.PreferencesProto.PreferenceMap;
import androidx.datastore.preferences.PreferencesProto.Value;
import androidx.datastore.preferences.PreferencesProto.StringSet;
import androidx.datastore.preferences.protobuf.ByteString;
import java.nio.file.*;
import java.util.*;

/** Synthetic-only oracle: no application paths or data. */
public final class Oracle {
    private static byte[] hex(String s) { return HexFormat.of().parseHex(s); }
    private static byte[] cat(byte[]... parts) {
        var out = new java.io.ByteArrayOutputStream();
        for (var p : parts) out.writeBytes(p);
        return out.toByteArray();
    }
    private static byte[] field(int tag, byte[] body) {
        if (body.length >= 128) throw new IllegalArgumentException("synthetic field too long");
        return cat(new byte[]{(byte)tag, (byte)body.length}, body);
    }
    private static byte[] entry(String key, byte[] value) {
        return field(10, cat(field(10, key.getBytes(java.nio.charset.StandardCharsets.UTF_8)), field(18, value)));
    }
    private static void record(Path dir, String name, byte[] bytes, StringBuilder results) throws Exception {
        Files.write(dir.resolve(name + ".input.bin"), bytes);
        try {
            var parsed = PreferenceMap.parseFrom(bytes);
            Files.write(dir.resolve(name + ".jvm.bin"), parsed.toByteArray());
            results.append(name).append("\tOK");
            for (var e : parsed.getPreferencesMap().entrySet()) {
                var v = e.getValue();
                results.append("\t").append(e.getKey()).append(":").append(v.getValueCase());
                switch (v.getValueCase()) {
                    case BOOLEAN -> results.append(":").append(v.getBoolean());
                    case INTEGER -> results.append(":").append(v.getInteger());
                    case LONG -> results.append(":").append(v.getLong());
                    case FLOAT -> results.append(":").append(Integer.toUnsignedString(Float.floatToRawIntBits(v.getFloat()), 16));
                    case DOUBLE -> results.append(":").append(Long.toUnsignedString(Double.doubleToRawLongBits(v.getDouble()), 16));
                    case STRING -> results.append(":").append(HexFormat.of().formatHex(v.getString().getBytes(java.nio.charset.StandardCharsets.UTF_8)));
                    case STRING_SET -> results.append(":").append(v.getStringSet().getStringsList());
                    case BYTES -> results.append(":").append(HexFormat.of().formatHex(v.getBytes().toByteArray()));
                    default -> { }
                }
            }
            results.append("\n");
        } catch (androidx.datastore.preferences.protobuf.InvalidProtocolBufferException e) {
            results.append(name).append("\tREJECT\n");
        }
    }
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]); Files.createDirectories(dir);
        StringBuilder results = new StringBuilder();
        var m = PreferenceMap.newBuilder();
        m.putPreferences("a_bool", Value.newBuilder().setBoolean(false).build());
        m.putPreferences("b_int", Value.newBuilder().setInteger(Integer.MIN_VALUE).build());
        m.putPreferences("c_long", Value.newBuilder().setLong(Long.MIN_VALUE).build());
        m.putPreferences("d_float", Value.newBuilder().setFloat(Float.intBitsToFloat(0x7fc01234)).build());
        m.putPreferences("e_string", Value.newBuilder().setString("雪\u0000😀").build());
        m.putPreferences("f_set", Value.newBuilder().setStringSet(StringSet.newBuilder().addStrings("雪").addStrings("x").addStrings("雪")).build());
        m.putPreferences("g_double", Value.newBuilder().setDouble(Double.longBitsToDouble(0xfff8000000001234L)).build());
        m.putPreferences("h_bytes", Value.newBuilder().setBytes(ByteString.copyFrom(hex("00ff80"))).build());
        record(dir, "all-types", m.build().toByteArray(), results);
        m.putPreferences("f_set", Value.newBuilder().setStringSet(StringSet.newBuilder().addStrings("雪").addStrings("x")).build());
        record(dir, "deduplicated-all-types", m.build().toByteArray(), results);
        m.putPreferences("f_set", Value.newBuilder().setStringSet(StringSet.newBuilder().addStrings("x").addStrings("雪")).build());
        record(dir, "canonical-all-types", m.build().toByteArray(), results);
        var extremes = PreferenceMap.newBuilder();
        extremes.putPreferences("a_int", Value.newBuilder().setInteger(Integer.MAX_VALUE).build());
        extremes.putPreferences("b_long", Value.newBuilder().setLong(Long.MAX_VALUE).build());
        extremes.putPreferences("c_minus", Value.newBuilder().setInteger(-1).build());
        extremes.putPreferences("d_float", Value.newBuilder().setFloat(-0.0f).build());
        extremes.putPreferences("e_double", Value.newBuilder().setDouble(Double.POSITIVE_INFINITY).build());
        record(dir, "extremes", extremes.build().toByteArray(), results);
        record(dir, "duplicate-map", cat(entry("x", hex("1801")), entry("x", hex("1802"))), results);
        record(dir, "duplicate-oneof", entry("x", hex("18012a017a0801")), results);
        record(dir, "merge-set", entry("x", hex("32030a016132030a0162")), results);
        record(dir, "replace-set", entry("x", hex("32030a0161180132030a0162")), results);
        record(dir, "duplicate-entry-value", field(10, cat(field(10, hex("78")), field(18, hex("32030a0161")), field(18, hex("32030a0162")))), results);
        record(dir, "duplicate-entry-key", field(10, cat(field(10, hex("61")), field(10, hex("62")), field(18, hex("1801")))), results);
        record(dir, "missing-key", field(10, field(18, hex("1801"))), results);
        record(dir, "missing-value", field(10, field(10, hex("78"))), results);
        record(dir, "empty", new byte[0], results);
        record(dir, "unknown-top", cat(entry("x", hex("1801")), hex("980607")), results);
        record(dir, "unknown-value", entry("x", hex("18014807")), results);
        record(dir, "future-only", entry("x", hex("4807")), results);
        record(dir, "unknown-set", entry("x", hex("32050a01611001")), results);
        record(dir, "unknown-entry", field(10, cat(field(10, hex("78")), field(18, hex("1801")), hex("1807"))), results);
        record(dir, "unknown-group", cat(entry("x", hex("1801")), hex("53080754")), results);
        record(dir, "wrong-known-wire", cat(entry("x", hex("1801")), hex("0801")), results);
        record(dir, "bool-noncanonical", entry("x", hex("088002")), results);
        record(dir, "int-truncation", entry("x", hex("18ffffffff0f")), results);
        record(dir, "invalid-utf8", entry("x", hex("2a01ff")), results);
        record(dir, "invalid-key", field(10, cat(field(10, hex("ff")), field(18, hex("1801")))), results);
        record(dir, "invalid-set", entry("x", hex("32030a01ff")), results);
        record(dir, "truncated", hex("0a050a0178"), results);
        record(dir, "invalid-tag", hex("00"), results);
        record(dir, "bad-varint", hex("8080808080808080808080"), results);
        record(dir, "unclosed-group", hex("53"), results);
        recordP1(dir, results);
        Files.writeString(dir.resolve("results.tsv"), results);
    }
    private static void recordP1(Path dir, StringBuilder results) throws Exception {
        byte[] unset = field(10, field(10, hex("78")));
        byte[] valid = entry("x", hex("1801"));
        // Exact reviewer example; missing first Value is resolved by last key.
        record(dir, "duplicate-unset-valid", hex("0a030a01780a070a017812021801"), results);
        record(dir, "duplicate-valid-unset", cat(valid, unset), results);
        record(dir, "duplicate-future-valid", cat(entry("x", hex("4807")), valid), results);
        byte[] unknownUnset = field(10, cat(field(10, hex("78")), hex("1807")));
        record(dir, "duplicate-unknown-unset-valid", cat(unknownUnset, valid), results);
        byte[] emptyValues = cat(field(18, new byte[0]), field(18, new byte[0]));
        record(dir, "repeated-empty-values-valid", field(10, cat(field(10, hex("78")), emptyValues, field(18, hex("1801")))), results);
        record(dir, "valid-repeated-empty-values", field(10, cat(field(10, hex("78")), field(18, hex("1801")), emptyValues)), results);
        byte[] emptyEntries = cat(field(10, new byte[0]), field(10, new byte[0]));
        record(dir, "repeated-empty-entries-valid", cat(emptyEntries, entry("", hex("1801"))), results);
        record(dir, "valid-repeated-empty-entries", cat(entry("", hex("1801")), emptyEntries), results);
        record(dir, "unset-other-key-valid", cat(unset, entry("y", hex("1801"))), results);
    }
}
