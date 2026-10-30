package com.hippo.ehviewer.coil

import coil3.Extras
import coil3.getExtra
import coil3.intercept.Interceptor
import coil3.request.ImageRequest
import coil3.request.ImageResult
import coil3.request.SuccessResult
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.files.delete
import com.ehviewer.core.files.isDirectory
import com.ehviewer.core.files.isFile
import com.ehviewer.core.files.sendTo
import com.ehviewer.core.files.toUri
import com.hippo.ehviewer.EhApplication.Companion.thumbCache
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.bridge.files.executeDownloadedThumbnail
import com.hippo.ehviewer.bridge.files.prepareDownloadedThumbnail
import com.hippo.ehviewer.bridge.files.usesDownloadedThumbnail
import com.hippo.ehviewer.client.getThumbKey
import com.hippo.ehviewer.download.downloadLocation

private val downloadInfoKey = Extras.Key<DownloadInfo?>(default = null)

fun ImageRequest.Builder.downloadInfo(info: DownloadInfo) = apply {
  extras[downloadInfoKey] = info
}

val ImageRequest.downloadInfo: DownloadInfo?
  get() = getExtra(downloadInfoKey)

object DownloadThumbInterceptor : Interceptor {
  override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
    val info = chain.request.downloadInfo
    if (usesDownloadedThumbnail(info != null, info?.dirname.isNullOrBlank())) {
      checkNotNull(info)
      val thumbKey = getThumbKey(chain.request.data as String)
      val (dir, filename) =
        prepareDownloadedThumbnail(
          info.thumbKey,
          thumbKey,
          update = {
            info.thumbKey = thumbKey
            EhDB.putGalleryInfo(info.galleryInfo)
          },
          directory = { downloadLocation / info.dirname!! },
        )
      val thumb = dir / filename
      val v1Thumb = dir / "thumb.jpg"
      return executeDownloadedThumbnail(
        localExists = { thumb.isFile },
        local = {
          val new = chain.request.newBuilder().data(thumb.toUri()).build()
          chain.withRequest(new).proceed()
        },
        remote = { chain.proceed() },
        success = { it is SuccessResult },
        directoryExists = { dir.isDirectory },
        differentOldPath = { thumb != v1Thumb },
        writeCache = {
          // Overwrite: recreating the SAF file can make immediate access fail.
          val key = requireNotNull(chain.request.memoryCacheKey)
          thumbCache.read(key) { data sendTo thumb }
        },
        deleteOld = { v1Thumb.delete() },
      )
    }
    return chain.proceed()
  }
}
