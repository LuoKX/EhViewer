package com.hippo.ehviewer.bridge.files;
import java.nio.file.*;
import java.util.*;

class ThumbnailFlowBridgeKt {
    private static native int[] nativeThumbnailFlowV1(int operation,int[] facts);
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for(int success=0;success<=1;success++)for(int directory=0;directory<=1;directory++){
            if(!Arrays.equals(new int[]{success==1&&directory==1?3:2,0},nativeThumbnailFlowV1(5,new int[]{success,directory})))throw new AssertionError("Remote phase mismatch");
        }
        if(!Arrays.equals(new int[]{2,1},nativeThumbnailFlowV1(6,new int[]{1})))throw new AssertionError("Cleanup mismatch");
        if(nativeThumbnailFlowV1(2,new int[]{3,46,0xD800,0xDC00})[0]!=1)throw new AssertionError("UTF16 suffix mismatch");
        for(int[] bad:new int[][]{null,new int[]{-1},new int[]{1,65536},new int[1048577]}){
            try{nativeThumbnailFlowV1(2,bad);throw new AssertionError("Invalid frame accepted");}catch(RuntimeException expected){}
        }
        System.out.println("Thumbnail flow JNI: write/cleanup phases, raw suffix and invalid frames passed");
    }
}
