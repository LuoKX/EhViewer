package com.hippo.ehviewer.bridge.gallery;
import java.nio.file.Path;
import java.util.Arrays;

class FavoriteBatchBridgeKt {
    private static native int[] nativeFavoriteBatchV1(int operation,int left,int right);
    private static void check(int op,int left,int right,int... expected) {
        if(!Arrays.equals(expected,nativeFavoriteBatchV1(op,left,right)))throw new AssertionError("Favorite intent mismatch");
    }
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        int cases=0;
        for(int source:new int[]{Integer.MIN_VALUE,-3,-2,-1,0,5,9,10,Integer.MAX_VALUE}) {
            check(0,source,0,source==-2?1:2,1);check(3,source,0,source==-2?1:0,-1);cases+=2;
            for(int destination:new int[]{-2,0,1,2,3,4,5,6,7,8,9}) {
                int action=source==destination?0:source==-2?3:destination==-2?4:5;
                check(1,source,destination,action,action==0?0:1);cases++;
            }
        }
        for(int size=1;size<=11;size++)for(int index=0;index<size;index++){check(2,index,size,index==0?-2:index-1);cases++;}
        for(int[] request:new int[][]{{9,0,0},{0,0,1},{1,0,-1},{1,0,10},{2,-1,11},{2,11,11},{2,0,12},{3,0,1}}) {
            try{nativeFavoriteBatchV1(request[0],request[1],request[2]);throw new AssertionError("Invalid favorite request accepted");}catch(RuntimeException expected){}
        }
        System.out.println("Favorite batch JNI: "+cases+" Java intent/menu/overlay cases and invalid inputs passed");
    }
}
