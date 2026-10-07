import androidx.datastore.preferences.PreferencesProto.PreferenceMap;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.Arrays;

/** Bounded synthetic host-JVM oracle; never reads application data. */
public final class Oracle {
    private static final int[] TAIL = {0, 0x7f, 0x80, 0x8f, 0x90, 0x9f, 0xa0, 0xbf, 0xc0, 0xff};
    private static int count;
    private static byte[] field(int tag, byte[] body) {
        if (body.length > 120) throw new IllegalArgumentException();
        byte[] result = new byte[body.length + 2];
        result[0] = (byte) tag; result[1] = (byte) body.length;
        System.arraycopy(body, 0, result, 2, body.length);
        return result;
    }
    private static byte[] cat(byte[] a, byte[] b) {
        byte[] result = Arrays.copyOf(a, a.length + b.length);
        System.arraycopy(b, 0, result, a.length, b.length); return result;
    }
    private static byte[] map(byte[] key, byte[] value) {
        return field(10, cat(field(10, key), field(18, value)));
    }
    private static void record(DataOutputStream out, byte[] bytes) throws Exception {
        String expected = new String(bytes, StandardCharsets.UTF_8);
        // Exercise actual generated map value/set parsing, not a decoder stand-in.
        String value = PreferenceMap.parseFrom(map(new byte[]{'x'}, field(42, bytes)))
            .getPreferencesOrThrow("x").getString();
        var set = PreferenceMap.parseFrom(map(new byte[]{'x'}, field(50, cat(field(10, bytes), field(10, bytes)))))
            .getPreferencesOrThrow("x").getStringSet().getStringsList();
        if (!expected.equals(value) || set.size() != 2 || !expected.equals(set.get(0)) || !expected.equals(set.get(1)))
            throw new IllegalStateException();
        byte[] canonical = expected.getBytes(StandardCharsets.UTF_8);
        if (bytes.length > 64 || expected.length() > 64 || canonical.length > 192) throw new IllegalStateException();
        out.writeByte(bytes.length); out.write(bytes);
        out.writeByte(expected.length());
        for (int i = 0; i < expected.length(); i++) out.writeChar(expected.charAt(i));
        out.writeShort(canonical.length); out.write(canonical);
        count++;
    }
    private static void checkOld(Path directory) throws Exception {
        for (String name : new String[]{"invalid-utf8", "invalid-set"}) {
            byte[] input = Files.readAllBytes(directory.resolve(name + ".input.bin"));
            byte[] frozen = Files.readAllBytes(directory.resolve(name + ".jvm.bin"));
            if (!Arrays.equals(PreferenceMap.parseFrom(input).toByteArray(), frozen)) throw new IllegalStateException();
        }
        byte[] invalidKey = Files.readAllBytes(directory.resolve("invalid-key.input.bin"));
        boolean rejected = false;
        try { PreferenceMap.parseFrom(invalidKey); }
        catch (androidx.datastore.preferences.protobuf.InvalidProtocolBufferException ignored) { rejected = true; }
        if (!rejected) throw new IllegalStateException();
    }
    private static void generate(Path output, Path old) throws Exception {
        checkOld(old);
        try (var out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(output)))) {
            out.writeInt(0x4a553031); // JU01; records end with 0xff sentinel plus count.
            record(out, new byte[0]);
            for (int a = 0; a < 256; a++) record(out, new byte[]{(byte)a});
            for (int a = 0; a < 256; a++) for (int b = 0; b < 256; b++)
                record(out, new byte[]{(byte)a, (byte)b});
            // Every 3/4-byte lead and second byte, boundary third/fourth bytes.
            for (int a = 0xe0; a <= 0xef; a++) for (int b = 0; b < 256; b++) for (int c : TAIL)
                record(out, new byte[]{(byte)a, (byte)b, (byte)c});
            for (int a = 0xf0; a <= 0xf4; a++) for (int b = 0; b < 256; b++) for (int c : TAIL) {
                record(out, new byte[]{(byte)a, (byte)b, (byte)c}); // truncated four-byte prefixes
                for (int d : TAIL) record(out, new byte[]{(byte)a, (byte)b, (byte)c, (byte)d});
            }
            // Exhaust every surrogate encoding, including surrogate range edges.
            for (int b = 0xa0; b <= 0xbf; b++) for (int c = 0x80; c <= 0xbf; c++)
                record(out, new byte[]{(byte)0xed, (byte)b, (byte)c});
            byte[][] parts = { {}, {0}, {0x41}, {(byte)0x80}, {(byte)0xff}, {(byte)0xc0},
                {(byte)0xc2}, {(byte)0xc2,(byte)0x80}, {(byte)0xdf,(byte)0xbf},
                {(byte)0xe0,(byte)0xa0,(byte)0x80}, {(byte)0xed,(byte)0x9f,(byte)0xbf},
                {(byte)0xed,(byte)0xa0}, {(byte)0xed,(byte)0xa0,(byte)0x80},
                {(byte)0xef,(byte)0xbf,(byte)0xbf}, {(byte)0xf0,(byte)0x90,(byte)0x80,(byte)0x80},
                {(byte)0xf4,(byte)0x8f,(byte)0xbf,(byte)0xbf}, {(byte)0xf4,(byte)0x90,(byte)0x80,(byte)0x80},
                {(byte)0xf0,(byte)0x90}, {(byte)0xf0,(byte)0x90,(byte)0x80}, {(byte)0xf5}, {(byte)0xfe}};
            for (byte[] a : parts) for (byte[] b : parts) for (byte[] c : parts)
                record(out, cat(cat(a, b), c));
            out.writeByte(255); out.writeInt(count);
        }
    }
    public static void main(String[] args) {
        try {
            if (args.length != 2) throw new IllegalArgumentException();
            generate(Path.of(args[0]), Path.of(args[1]));
            System.out.println("Synthetic JVM oracle replay passed: " + count + " cases; strict key rejected");
        } catch (Exception ignored) {
            System.err.println("FAIL: synthetic JVM oracle unavailable or malformed");
            System.exit(1);
        }
    }
}
