package com.hippo.ehviewer.bridge.files;

import java.nio.file.Path;
import java.text.DecimalFormatSymbols;
import java.util.*;
import java.util.regex.Pattern;

class DownloadFilesBridgeKt {
    private static native char[] nativePageFilenameV1(int index, int zero, char[] extension);
    private static native String nativeExpectedImageHashV1(char[] url);
    private static native int[] nativePostArchivePagesV1(boolean[] matches);
    private interface Probe { void run(); }
    private static void rejects(Probe action) {
        try { action.run(); } catch (RuntimeException expected) { return; }
        throw new AssertionError("Invalid download input accepted");
    }
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        int cases = 0;
        for (String tag : new String[] {"en-US", "ar-EG", "fa-IR", "bn-BD", "th-TH-u-nu-thai", "en-US-u-nu-fullwide"}) {
            Locale locale = Locale.forLanguageTag(tag);
            int zero = DecimalFormatSymbols.getInstance(locale).getZeroDigit();
            for (int index : new int[] {0, -1, -2, 99_999_998, 99_999_999, Integer.MIN_VALUE, Integer.MAX_VALUE}) {
                for (String extension : new String[] {"", "jpg", "png.tmp", "\ud800\udc01\ud800"}) {
                    String expected = String.format(locale, "%08d.%s", index + 1, extension);
                    String actual = new String(nativePageFilenameV1(index, zero, extension.toCharArray()));
                    if (!actual.equals(expected)) throw new AssertionError("Locale filename mismatch: " + tag);
                    cases++;
                }
            }
        }
        String hash = "0123456789abcdef0123456789abcdef01234567";
        Pattern grammar = Pattern.compile("/h/([0-9a-f]{40})");
        for (String url : new String[] {"", "/h/" + hash, "/H/" + hash, "/h/" + hash.toUpperCase(Locale.ROOT), "/h/" + hash + "f", "/h/invalid/h/" + hash, "\ud800/h/" + hash, "/h/" + hash.substring(1)}) {
            var matcher = grammar.matcher(url);
            String expected = matcher.find() ? matcher.group(1) : null;
            if (!Objects.equals(expected, nativeExpectedImageHashV1(url.toCharArray()))) throw new AssertionError("Hash grammar mismatch");
        }
        for (boolean[] matches : new boolean[][] {{}, {false}, {true}, {false, true, false, true}, {true, true, true}}) {
            int[] expected = java.util.stream.IntStream.range(0, matches.length).filter(i -> matches[i]).toArray();
            if (!Arrays.equals(expected, nativePostArchivePagesV1(matches))) throw new AssertionError("Cleanup order changed");
        }
        rejects(() -> nativePageFilenameV1(0, -1, new char[0]));
        rejects(() -> nativePageFilenameV1(0, 65535, new char[0]));
        rejects(() -> nativePageFilenameV1(0, 48, new char[4097]));
        rejects(() -> nativeExpectedImageHashV1(new char[16385]));
        rejects(() -> nativePostArchivePagesV1(new boolean[100001]));
        System.out.println("Download files host JNI: " + cases + " locale filename oracles; hash grammar and cleanup order; limits rejected");
    }
}
