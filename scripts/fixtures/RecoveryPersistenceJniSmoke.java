package com.hippo.ehviewer.bridge.download;
import java.nio.ByteBuffer;
import java.nio.file.*;
import java.util.*;

class RecoveryPersistenceBridgeKt {
    private static native byte[] nativeRecoveryPersistenceV1(ByteBuffer request,int size);
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for(String name:List.of("artists","stage")) {
            byte[] input=HexFormat.of().parseHex(Files.readString(Path.of(args[1],"recovery_"+name+"_request.cbor.hex")).trim());
            byte[] output=HexFormat.of().parseHex(Files.readString(Path.of(args[1],"recovery_"+name+"_response.cbor.hex")).trim());
            ByteBuffer buffer=ByteBuffer.allocateDirect(input.length).put(input);
            if(!Arrays.equals(output,nativeRecoveryPersistenceV1(buffer,input.length)))throw new AssertionError("Recovery golden mismatch");
            for(int size:new int[]{-1,0,input.length+1,16777217}) {
                try{nativeRecoveryPersistenceV1(buffer,size);throw new AssertionError("Size accepted");}catch(RuntimeException expected){}
            }
            try{nativeRecoveryPersistenceV1(ByteBuffer.wrap(input),input.length);throw new AssertionError("Heap accepted");}catch(RuntimeException expected){}
        }
        try{nativeRecoveryPersistenceV1(ByteBuffer.allocateDirect(1).put((byte)255),1);throw new AssertionError("CBOR accepted");}catch(RuntimeException expected){}
        System.out.println("Recovery persistence JNI: raw UTF16 spans, staging golden and invalid frames passed");
    }
}
