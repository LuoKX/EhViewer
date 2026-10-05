package com.hippo.ehviewer.spider

import com.ehviewer.core.files.read
import com.ehviewer.core.files.write
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.reader.MAX_SPIDER_INFO_BYTES
import com.hippo.ehviewer.bridge.reader.SpiderInfoCodecKind
import com.hippo.ehviewer.bridge.reader.SpiderInfoCodecRequest
import com.hippo.ehviewer.bridge.reader.SpiderInfoData
import com.hippo.ehviewer.bridge.reader.codecSpiderInfo
import com.hippo.ehviewer.coil.edit
import com.hippo.ehviewer.coil.read
import com.hippo.ehviewer.ktbuilder.diskCache
import java.util.Collections
import kotlinx.io.IOException
import moe.tarsin.coroutines.runSuspendCatching
import okio.Path
import okio.Path.Companion.toOkioPath
import splitties.init.appCtx

class SpiderInfo(
  val gid: Long,
  val token: String,
  val pages: Int,
  pTokenMap: MutableMap<Int, String> = hashMapOf(),
  var previewPerPage: Int = -1,
) {
  val pTokenMap: MutableMap<Int, String> = Collections.synchronizedMap(pTokenMap)

  internal fun snapshot() =
    synchronized(pTokenMap) {
      SpiderInfoData(gid, token, pages, pTokenMap.toMap(), previewPerPage)
    }
}

fun SpiderInfo.write(file: Path) {
  val plan =
    codecSpiderInfo(SpiderInfoCodecRequest(kind = SpiderInfoCodecKind.ENCODE, info = snapshot()))
  file.write { write(checkNotNull(plan.bytes)) }
}

fun SpiderInfo.saveToCache() {
  runSuspendCatching {
    spiderInfoCache.edit(gid.toString()) { write(data) }
  }
    .onFailure { logcat(it) }
}

private val spiderInfoCache by lazy {
  diskCache {
    directory(appCtx.cacheDir.toOkioPath() / "spider_info_v2_1")
    maxSizeBytes(20 * 1024 * 1024)
  }
}

fun readFromCache(gid: Long): SpiderInfo? =
  spiderInfoCache.read(gid.toString()) {
    readSpiderInfo(data, allowLegacy = false)
  }

fun readCompatFromPath(file: Path): SpiderInfo? = readSpiderInfo(file, allowLegacy = true)

private fun readSpiderInfo(file: Path, allowLegacy: Boolean): SpiderInfo? {
  val bytes =
    try {
      file.read {
        val bounded = ByteArray(MAX_SPIDER_INFO_BYTES + 1)
        var count = 0
        while (count < bounded.size) {
          val read = readAtMostTo(bounded, count, bounded.size)
          if (read == -1) break
          count += read
        }
        if (count > MAX_SPIDER_INFO_BYTES)
          throw IOException("SpiderInfo metadata exceeds resource budget")
        bounded.copyOf(count)
      }
    } catch (failure: IOException) {
      logcat("SpiderInfo", failure)
      return null
    }
  if (bytes.isEmpty()) return null
  val plan =
    try {
      codecSpiderInfo(
        SpiderInfoCodecRequest(
          kind = SpiderInfoCodecKind.DECODE,
          bytes = bytes,
          allowLegacy = allowLegacy,
        )
      )
    } catch (failure: RustFailureException) {
      logcat("SpiderInfo", failure)
      return null
    }
  return checkNotNull(plan.info).let {
    SpiderInfo(it.gid, it.token, it.pages, it.pTokenMap.toMutableMap(), it.previewPerPage)
  }
}
