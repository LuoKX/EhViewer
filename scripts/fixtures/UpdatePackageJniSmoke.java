package com.hippo.ehviewer.bridge.updater;

import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;

class UpdatePackageBridgeKt {
    private static native long nativeExtractUpdateApkV1(int source, int target);
    private static int fd(RandomAccessFile file) throws Exception {
        var field = FileDescriptor.class.getDeclaredField("fd"); field.setAccessible(true);
        return field.getInt(file.getFD());
    }
    private static void zip(Path path, String... names) throws IOException {
        try (var output = new ZipOutputStream(Files.newOutputStream(path))) {
            for (String name : names) { output.putNextEntry(new ZipEntry(name)); if (!name.endsWith("/")) output.write(("payload-" + name).getBytes(java.nio.charset.StandardCharsets.UTF_8)); output.closeEntry(); }
        }
    }
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        Path root = Files.createTempDirectory("eh-update-jni");
        try {
            Path input = root.resolve("input.zip"), output = root.resolve("output.apk");
            zip(input, "docs/", "README", "build/app.apk");
            try (var source = new RandomAccessFile(input.toFile(), "r"); var target = new RandomAccessFile(output.toFile(), "rw")) {
                source.seek(5);
                long count = nativeExtractUpdateApkV1(fd(source), fd(target));
                if (count != target.length() || source.getFilePointer() != 5 || !Files.readString(output).equals("payload-build/app.apk")) throw new AssertionError("APK extraction/FD position failed");
                long length = source.length();
                try { nativeExtractUpdateApkV1(fd(source), fd(source)); throw new AssertionError("Aliased source/output accepted"); } catch (RuntimeException expected) { }
                if (source.length() != length) throw new AssertionError("Source truncated");
            }
            for (String[] names : new String[][] {{}, {"README"}, {"one.apk", "two.apk"}, {"../unsafe.apk"}}) {
                zip(input, names);
                try (var source = new RandomAccessFile(input.toFile(), "r"); var target = new RandomAccessFile(output.toFile(), "rw")) {
                    target.writeUTF("old staging bytes");
                    try { nativeExtractUpdateApkV1(fd(source), fd(target)); throw new AssertionError("Invalid update accepted"); } catch (RuntimeException expected) { }
                    if (target.length() != 0) throw new AssertionError("Failed output not cleared");
                    if (!source.getFD().valid() || !target.getFD().valid()) throw new AssertionError("Caller descriptor closed");
                }
            }
            try { nativeExtractUpdateApkV1(-1, -1); throw new AssertionError("Negative descriptors accepted"); } catch (RuntimeException expected) { }
        } finally {
            try (var paths = Files.walk(root)) { for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) Files.delete(path); }
        }
        System.out.println("Update package host JNI: unique APK selection, actual FD extraction, source offset/ownership, alias and invalid-container rejection");
    }
}
