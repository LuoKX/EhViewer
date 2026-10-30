package com.hippo.ehviewer.bridge.reader;
import java.nio.file.*;
import java.util.*;

class ReaderNavigationBridgeKt {
    private static native int[] nativeReaderNavigationV1(int operation,int[] facts);
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for(int key=0;key<=4;key++)for(int down=0;down<=1;down++)for(int volume=0;volume<=1;volume++)for(int invert=0;invert<=1;invert++){
            int expected=down==0?0:key<3?key:volume==0?0:(key==3)==(invert==0)?1:2;
            if(nativeReaderNavigationV1(5,new int[]{key,down,volume,invert})[0]!=expected)throw new AssertionError("Key matrix mismatch");
        }
        for(float f:new float[]{Float.NaN,1f,Math.nextUp(1f),Float.POSITIVE_INFINITY}){
            if(nativeReaderNavigationV1(4,new int[]{1,Float.floatToRawIntBits(f)})[0]!=(f>1f?1:0))throw new AssertionError("Pan threshold mismatch");
        }
        if(!Arrays.equals(new int[]{2,0,0,1},nativeReaderNavigationV1(1,new int[]{0})))throw new AssertionError("Default mode mismatch");
        for(int[] bad:new int[][]{null,new int[]{0,-1},new int[]{0,1,65536},new int[1048577]}){
            try{nativeReaderNavigationV1(1,bad);throw new AssertionError("Invalid frame accepted");}catch(RuntimeException expected){}
        }
        System.out.println("Reader navigation JNI: key matrix, Float edges, mode and invalid frames passed");
    }
}
