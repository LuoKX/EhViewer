package com.hippo.ehviewer.bridge.gallery;
import java.nio.ByteBuffer;
import java.nio.file.*;
import java.util.*;

class CatalogStateBridgeKt {
    private static native int[] nativeCatalogNumbersV1(int operation, int left, int right);
    private static native byte[] nativeCatalogNavigationV1(ByteBuffer request, int size);
    private static void check(int op, int a, int b, int... expected) {
        if (!Arrays.equals(expected, nativeCatalogNumbersV1(op, a, b))) throw new AssertionError("Numeric mismatch " + op);
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        int[] edges = {Integer.MIN_VALUE, -2001, -20, -1, 0, 1, 2, 9, 10, 19, 20, 21, 40, 199, 200, 201, 1000, 2000, Integer.MAX_VALUE};
        int cases = 0;
        for (int a : edges) {
            check(1,a,0,Math.max(a-1,0)); check(2,a,0,a==0?0:a+1);
            check(3,a,0,Math.max(0,Math.min(1000,a))); check(4,a,0,Math.max(0,Math.min(2000,a)));
            check(6,a,0,a>=1&&a<=200?1:0); check(7,a,0,a-1>0?1:0,a-1,a+1<=200?1:0,a+1); check(8,a,0,a+1);
            cases += 7;
            for (int b : edges) {
                check(0,a,b,a^b);
                check(5,a,b,b==0?0:a==0?(b>=10?0:1):(a<=Math.min(b/2,b-20)?0:2));
                cases += 2;
            }
        }
        check(9,0,0,200);
        byte[] input = HexFormat.of().parseHex(Files.readString(Path.of(args[1], "catalog_state_request.cbor.hex")).trim());
        byte[] output = HexFormat.of().parseHex(Files.readString(Path.of(args[1], "catalog_state_response.cbor.hex")).trim());
        ByteBuffer buffer = ByteBuffer.allocateDirect(input.length).put(input);
        if (!Arrays.equals(output, nativeCatalogNavigationV1(buffer,input.length))) throw new AssertionError("Navigation wire mismatch");
        for (int size : new int[]{-1,0,input.length+1,131073}) {
            try { nativeCatalogNavigationV1(buffer,size); throw new AssertionError("Invalid size accepted"); } catch (RuntimeException expected) { }
        }
        try { nativeCatalogNavigationV1(ByteBuffer.wrap(input),input.length); throw new AssertionError("Heap accepted"); } catch (RuntimeException expected) { }
        try { nativeCatalogNumbersV1(99,0,0); throw new AssertionError("Opcode accepted"); } catch (RuntimeException expected) { }
        System.out.println("Catalog JNI: " + cases + " JVM arithmetic cases, golden and invalid boundaries passed");
    }
}
