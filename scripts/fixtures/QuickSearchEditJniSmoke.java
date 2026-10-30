package com.hippo.ehviewer.bridge.gallery;
import java.nio.ByteBuffer;
import java.nio.file.*;
import java.util.*;

class QuickSearchEditBridgeKt {
    private static native byte[] nativeQuickSearchEditV1(ByteBuffer request,int size);
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for(String name:new String[]{"quick_search_check","quick_search_create"}) {
            byte[] input=HexFormat.of().parseHex(Files.readString(Path.of(args[1],name+"_request.cbor.hex")).trim());
            byte[] output=HexFormat.of().parseHex(Files.readString(Path.of(args[1],name+"_response.cbor.hex")).trim());
            ByteBuffer buffer=ByteBuffer.allocateDirect(input.length).put(input);
            if(!Arrays.equals(output,nativeQuickSearchEditV1(buffer,input.length)))throw new AssertionError("Quick search golden mismatch");
            byte[] untouched=new byte[input.length];buffer.position(0);buffer.get(untouched);
            if(!Arrays.equals(input,untouched))throw new AssertionError("Request mutated");
            for(int size:new int[]{-1,0,input.length+1,16777217}) {
                try{nativeQuickSearchEditV1(buffer,size);throw new AssertionError("Invalid size accepted");}catch(RuntimeException expected){}
            }
            try{nativeQuickSearchEditV1(ByteBuffer.wrap(input),input.length);throw new AssertionError("Heap accepted");}catch(RuntimeException expected){}
        }
        try{nativeQuickSearchEditV1(ByteBuffer.allocateDirect(1).put((byte)255),1);throw new AssertionError("CBOR accepted");}catch(RuntimeException expected){}
        System.out.println("Quick search JNI: both phase goldens, raw UTF16, immutable input and invalid boundaries passed");
    }
}
