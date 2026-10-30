package com.hippo.ehviewer.bridge.library;
import java.nio.ByteBuffer;
import java.nio.file.*;
import java.util.*;

class ListOrderBridgeKt {
    private static native byte[] nativeListOrderV1(ByteBuffer request,int size);
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        byte[] input=HexFormat.of().parseHex(Files.readString(Path.of(args[1],"list_order_finish_request.cbor.hex")).trim());
        byte[] output=HexFormat.of().parseHex(Files.readString(Path.of(args[1],"list_order_finish_response.cbor.hex")).trim());
        ByteBuffer buffer=ByteBuffer.allocateDirect(input.length).put(input);
        if(!Arrays.equals(output,nativeListOrderV1(buffer,input.length)))throw new AssertionError("List order golden mismatch");
        for(int size:new int[]{-1,0,input.length+1,2097153}) {
            try{nativeListOrderV1(buffer,size);throw new AssertionError("Invalid size accepted");}catch(RuntimeException expected){}
        }
        try{nativeListOrderV1(ByteBuffer.wrap(input),input.length);throw new AssertionError("Heap accepted");}catch(RuntimeException expected){}
        try{nativeListOrderV1(ByteBuffer.allocateDirect(1).put((byte)255),1);throw new AssertionError("CBOR accepted");}catch(RuntimeException expected){}
        System.out.println("List order JNI: exact positional golden and invalid boundaries passed");
    }
}
