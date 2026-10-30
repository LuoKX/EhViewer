package com.hippo.ehviewer.bridge.updater

import android.os.ParcelFileDescriptor
import java.io.File

internal fun extractUpdateApk(source: File, output: File) {
  ParcelFileDescriptor.open(source, ParcelFileDescriptor.MODE_READ_ONLY).use { input ->
    ParcelFileDescriptor.open(output, ParcelFileDescriptor.MODE_READ_WRITE).use { target ->
      val size = nativeExtractUpdateApkV1(input.fd, target.fd)
      require(size in 1..512L * 1024 * 1024 && output.length() == size) {
        "Invalid Rust update output"
      }
    }
  }
}

private external fun nativeExtractUpdateApkV1(sourceFd: Int, outputFd: Int): Long
