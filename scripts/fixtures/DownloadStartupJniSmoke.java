package com.hippo.ehviewer.bridge.download;
import java.nio.file.Path;
import java.util.Arrays;

class DownloadStartupBridgeKt {
    private static native int[] nativeNormalizeDownloadStatesV1(int[] states);
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        int[] input = new int[100_000];
        for (int i = 0; i < input.length; i++) input[i] = i - 50_000;
        input[0] = Integer.MIN_VALUE; input[1] = Integer.MAX_VALUE;
        int[] expected = Arrays.stream(input).map(v -> v == 1 || v == 2 ? 0 : v).toArray();
        int[] actual = nativeNormalizeDownloadStatesV1(input);
        if (!Arrays.equals(expected, actual) || !Arrays.equals(actual, nativeNormalizeDownloadStatesV1(actual))) throw new AssertionError("Startup state mismatch");
        if (input[50001] != 1 || input[50002] != 2) throw new AssertionError("Input mutated");
        try { nativeNormalizeDownloadStatesV1(new int[100_001]); throw new AssertionError("Oversized input accepted"); } catch (RuntimeException expectedError) { }
        try { nativeNormalizeDownloadStatesV1(null); throw new AssertionError("Null accepted"); } catch (RuntimeException expectedError) { }
        System.out.println("Download startup JNI: 100000 raw states and rejection boundaries passed");
    }
}
