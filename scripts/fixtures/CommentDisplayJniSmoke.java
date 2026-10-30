package com.hippo.ehviewer.bridge.gallery;

import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

class CommentDisplayBridgeKt {
    private static native int[] nativeCommentLinksV1(char[] text, byte[] digits);
    private static native int[] nativeCommentImageDimensionsV1(char[] text, byte[] digits);
    private static final Pattern DIGIT = Pattern.compile("\\d");
    private static final Pattern LINK = Pattern.compile("(http|https)://[a-z0-9A-Z%-]+(\\.[a-z0-9A-Z%-]+)+(:\\d{1,5})?(/[a-zA-Z0-9-_~:#@!&',;=%/*.?+$\\[\\]()]+)?/?");
    private static final Pattern IMAGE = Pattern.compile("(?:[^-]+-){2}(\\d+)-(\\d+)-[^_]+_([^.]+)");
    private static byte[] facts(String text) {
        byte[] facts = new byte[text.length()];
        for (int i = 0; i < facts.length; i++) { int value = Character.digit(text.charAt(i), 10); facts[i] = (byte) (value < 0 ? 15 : value); }
        var matcher = DIGIT.matcher(text);
        while (matcher.find()) for (int i = matcher.start(); i < matcher.end(); i++) facts[i] |= 16;
        return facts;
    }
    private static void links(String text) {
        var expected = new ArrayList<Integer>();
        var matcher = LINK.matcher(text);
        while (matcher.find()) { expected.add(matcher.start()); expected.add(matcher.end()); }
        int[] result = nativeCommentLinksV1(text.toCharArray(), facts(text));
        if (!Arrays.equals(expected.stream().mapToInt(Integer::intValue).toArray(), result)) throw new AssertionError("Link range mismatch for fixture " + text + " expected=" + expected + " actual=" + Arrays.toString(result));
    }
    private static void image(String text) {
        int[] expected = null;
        boolean error = false;
        var matcher = IMAGE.matcher(text);
        if (matcher.find()) {
            try {
                int width = Integer.parseInt(matcher.group(1));
                int height = Integer.parseInt(matcher.group(2));
                int target;
                try { target = Integer.parseInt(matcher.group(3)); } catch (NumberFormatException ignored) { target = 200; }
                expected = new int[] {width, height, target, target / 2 * 3};
            } catch (NumberFormatException ignored) { error = true; }
        }
        try {
            int[] result = nativeCommentImageDimensionsV1(text.toCharArray(), facts(text));
            if (error || !Arrays.equals(expected, result)) throw new AssertionError("Image dimension mismatch for fixture " + text);
        } catch (NumberFormatException failure) { if (!error) throw new AssertionError("Unexpected number failure", failure); }
    }
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        String[] pieces = {" ", "😀", "\ud800", "http://a.b", "https://a.b.c/", ":123456/", ":١٢٣/", "HTTP://c.d", "http://a..b", "/[]()_~-!@$',;=+*", "...", "#c42", "\n", "http://a.b:9", "http://a.b:𝟘"};
        Random random = new Random(8721);
        for (char unit = 0; unit < 128; unit++) {
            links("http://a.b/a" + unit + "z");
            links("http://a" + unit + "b.c");
            links("http://a.b:" + String.valueOf(unit).repeat(6) + "/x");
        }
        for (int i = 0; i < 2000; i++) {
            StringBuilder text = new StringBuilder();
            for (int j = 0; j < 8; j++) text.append(pieces[random.nextInt(pieces.length)]);
            links(text.toString());
        }
        String[] values = {"0", "1", "123", "2147483647", "2147483648", "4294967299", "10000000000", "+11", "-11", "-2147483648", "١٢", "", "bad", "\ud800", "12.34"};
        for (String width : values) for (String height : values) for (String target : values)
            image("😀prefix-first-" + width + "-" + height + "-suffix_" + target + ".jpg");
        for (String text : new String[] {"", "a-b-100-200-c_401.jpg", "a-b-1-2-c_٣", "a--b-1-2-c_4", "a-b-1-2-c_3-a-b-4-5-c_6", "\ud800-b-1-2-c_3", "a.b-c-1-2-d_200"}) image(text);
        try { nativeCommentLinksV1(new char[1], new byte[0]); throw new AssertionError("Mismatched facts accepted"); } catch (RuntimeException expected) { }
        try { nativeCommentImageDimensionsV1(new char[128*1024+1], new byte[0]); throw new AssertionError("Oversized text accepted"); } catch (RuntimeException expected) { }
        System.out.println("Comment display host JNI: 2384 Java regex link cases and 3382 image/number cases; raw UTF16 and exceptions preserved");
    }
}
