// Host-only probe; same Kotlin file class and exact-sized JNI signature as production.
package com.hippo.ehviewer.bridge.gallery;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.HexFormat;

class GalleryRequestBridgeKt {
    private static native byte[] nativePlanGalleryRequestV1(ByteBuffer request, int size);
    private interface Probe { void run(); }
    private static void failure(Probe probe) {
        try { probe.run(); } catch (RuntimeException expected) {
            if (expected.getMessage().contains("secret-key")) {
                throw new AssertionError("Sensitive native diagnostic");
            }
            return;
        }
        throw new AssertionError("Native failure hidden");
    }
    private static byte[] rawUtf16(String value) {
        byte[] bytes = new byte[value.length() * 2];
        for (int i = 0; i < value.length(); i++) {
            bytes[i * 2] = (byte) value.charAt(i);
            bytes[i * 2 + 1] = (byte) (value.charAt(i) >>> 8);
        }
        return bytes;
    }
    private static byte[] rawField(byte[] response) {
        byte[] key = "wikiUrlUtf16".getBytes(StandardCharsets.US_ASCII);
        for (int i = 0; i + key.length < response.length; i++) {
            if (!Arrays.equals(key, Arrays.copyOfRange(response, i, i + key.length))) continue;
            int position = i + key.length;
            int header = response[position++] & 255;
            if ((header >>> 5) != 2) throw new AssertionError("Wiki target not CBOR bytes");
            int size = header & 31;
            if (size == 24) size = response[position++] & 255;
            else if (size == 25) {
                size = ((response[position] & 255) << 8) | (response[position + 1] & 255);
                position += 2;
            }
            if (size >= 26 && (header & 31) >= 26) throw new AssertionError("Unexpected size header");
            if (position + size > response.length) throw new AssertionError("Truncated raw field");
            return Arrays.copyOfRange(response, position, position + size);
        }
        throw new AssertionError("Missing raw wiki field");
    }
    private static String wikiTag(String name) {
        return switch (name) {
            case "navigation_wiki" -> "a b/+%#?中文😀\t\n";
            case "navigation_wiki_high" -> "\uD800";
            case "navigation_wiki_low" -> "\uDC00";
            case "navigation_wiki_pair_high" -> "\uD83D\uDE00\uD800";
            case "navigation_wiki_pair_low" -> "\uD83D\uDE00\uDC00";
            case "navigation_wiki_delimiters_space" -> " a/+%#?\uD800 \uDC00 ";
            default -> throw new AssertionError("Unknown wiki case");
        };
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        var root = Path.of(args[1]);
        for (String name : new String[] {
            "gallery_request", "navigation_favorites", "navigation_torrent",
            "navigation_tracker_null", "navigation_tracker_empty", "navigation_tracker_key",
            "navigation_wiki", "navigation_wiki_high", "navigation_wiki_low",
            "navigation_wiki_pair_high", "navigation_wiki_pair_low",
            "navigation_wiki_delimiters_space"
        }) {
            byte[] request = HexFormat.of().parseHex(
                Files.readString(root.resolve(name + "_request.cbor.hex")).trim());
            byte[] expected = HexFormat.of().parseHex(
                Files.readString(root.resolve(name + "_response.cbor.hex")).trim());
            var input = ByteBuffer.allocateDirect(request.length).put(request).flip();
            byte[] actual = nativePlanGalleryRequestV1(input, request.length);
            if (!Arrays.equals(actual, expected)) throw new AssertionError("Golden differs: " + name);
            if (name.startsWith("navigation_wiki")) {
                String oracle = "https://ehwiki.org/wiki/" + wikiTag(name).replace(' ', '_');
                if (!Arrays.equals(rawUtf16(oracle), rawField(actual))) {
                    throw new AssertionError("Frozen Java String oracle differs: " + name);
                }
                if (name.equals("navigation_wiki_high")) {
                    byte[] odd = Arrays.copyOf(request, request.length - 1);
                    odd[request.length - 3] = 0x41;
                    var malformed = ByteBuffer.allocateDirect(odd.length).put(odd).flip();
                    String error = new String(nativePlanGalleryRequestV1(malformed, odd.length), StandardCharsets.ISO_8859_1);
                    if (!error.contains("Invalid gallery wiki request shape")) {
                        throw new AssertionError("Odd UTF-16 bytes accepted");
                    }
                }
            }
            byte[] after = new byte[request.length]; input.duplicate().get(after);
            if (!Arrays.equals(after, request)) throw new AssertionError("Native mutated request");
            failure(() -> nativePlanGalleryRequestV1(input, -1));
            failure(() -> nativePlanGalleryRequestV1(input, 0));
            failure(() -> nativePlanGalleryRequestV1(input, request.length + 1));
            failure(() -> nativePlanGalleryRequestV1(input, 2_097_153));
            failure(() -> nativePlanGalleryRequestV1(ByteBuffer.wrap(request), request.length));
            if (name.equals("navigation_tracker_null")) {
                byte[] invalid = new String(request, StandardCharsets.ISO_8859_1)
                    .replace("trackerUrl", "secret-key").getBytes(StandardCharsets.ISO_8859_1);
                var malformed = ByteBuffer.allocateDirect(invalid.length).put(invalid).flip();
                failure(() -> nativePlanGalleryRequestV1(malformed, invalid.length));
                byte[] wrongVersion = request.clone();
                // Request's first field is "version" with its single-byte unsigned value.
                wrongVersion[9] = 2;
                var unsupported = ByteBuffer.allocateDirect(wrongVersion.length).put(wrongVersion).flip();
                String response = new String(nativePlanGalleryRequestV1(unsupported, wrongVersion.length), StandardCharsets.ISO_8859_1);
                if (!response.contains("Unsupported gallery request version")) {
                    throw new AssertionError("Unsupported version accepted");
                }
            }
        }
        System.out.println("Gallery navigation JNI: 12 exact responses, 6 raw wiki Java String oracles including lone units, odd raw shape rejected, immutable inputs, length/direct-buffer bounds, redacted enum failure, unsupported version");
    }
}
