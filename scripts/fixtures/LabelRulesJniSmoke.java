package com.hippo.ehviewer.bridge.download;
import java.nio.ByteBuffer;
import java.nio.file.*;
import java.util.*;

class LabelRulesBridgeKt {
    private static native byte[] nativeLabelRulesV1(ByteBuffer request, int size);
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for (String name : new String[]{"label_rules", "label_edit"}) {
        byte[] input = HexFormat.of().parseHex(Files.readString(Path.of(args[1], name+"_request.cbor.hex")).trim());
        byte[] output = HexFormat.of().parseHex(Files.readString(Path.of(args[1], name+"_response.cbor.hex")).trim());
        ByteBuffer buffer = ByteBuffer.allocateDirect(input.length).put(input);
        if (!Arrays.equals(output,nativeLabelRulesV1(buffer,input.length))) throw new AssertionError("Label wire mismatch");
        for (int size : new int[]{-1,0,input.length+1,8388609}) {
            try { nativeLabelRulesV1(buffer,size); throw new AssertionError("Invalid size accepted"); } catch (RuntimeException expected) { }
        }
        try { nativeLabelRulesV1(ByteBuffer.wrap(input),input.length); throw new AssertionError("Heap accepted"); } catch (RuntimeException expected) { }
        try { nativeLabelRulesV1(ByteBuffer.allocateDirect(1).put((byte)255),1); throw new AssertionError("Malformed CBOR accepted"); } catch (RuntimeException expected) { }
        }
        System.out.println("Label JNI: raw UTF16 golden and invalid boundaries passed");
    }
}
