package com.hippo.ehviewer.bridge.download;
import java.nio.file.Path;
import java.util.*;

class EntryActionsBridgeKt {
    private static native int[] nativeEntryActionV1(int operation, int left, int right, boolean[] facts);
    public static void main(String[] args) {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        for (int bits=0; bits<256; bits++) {
            boolean[] facts = new boolean[8];
            List<Integer> expected = new ArrayList<>();
            expected.add(Integer.bitCount(bits));
            for (int i=0;i<8;i++) { facts[i]=(bits&(1<<i))!=0; if(facts[i])expected.add(i); }
            for (int i=0;i<8;i++) if(!facts[i])expected.add(i);
            if (!Arrays.equals(expected.stream().mapToInt(i->i).toArray(),nativeEntryActionV1(0,0,0,facts))) throw new AssertionError("Partition mismatch");
        }
        for (int bits=0;bits<32;bits++) {
            boolean[] f=new boolean[5]; for(int i=0;i<5;i++) f[i]=(bits&(1<<i))!=0;
            boolean direct=f[0],used=false;
            if(!direct&&f[1]){direct=f[2]||f[3];used=direct;}
            if(!direct&&f[4]){direct=true;used=false;}
            if (!Arrays.equals(new int[]{direct?1:0,used?1:0},nativeEntryActionV1(1,0,0,f))) throw new AssertionError("Default mismatch");
        }
        try { nativeEntryActionV1(1,0,0,new boolean[0]); throw new AssertionError("Shape accepted"); } catch(RuntimeException expected) { }
        try { nativeEntryActionV1(0,0,0,new boolean[100001]); throw new AssertionError("Size accepted"); } catch(RuntimeException expected) { }
        System.out.println("Entry JNI: 256 partitions and 32 default choices passed");
    }
}
