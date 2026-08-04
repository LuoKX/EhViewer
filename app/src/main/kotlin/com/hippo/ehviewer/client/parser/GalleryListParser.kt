package com.hippo.ehviewer.client.parser

import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryPage as RustGalleryPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toGalleryListResult
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import moe.tarsin.coroutines.runSuspendCatching

object GalleryListParser {
  fun parse(body: ByteBuffer) = parse(body, Settings.useRustCore.value)

  internal fun parse(body: ByteBuffer, useRust: Boolean) = runSuspendCatching {
    if (useRust) {
      unmarshalParsingAs<ApiEnvelope<RustGalleryPage>>(
          body,
          ::parseGalleryInfoListV1,
        )
        .toGalleryListResult()
    } else {
      unmarshalParsingAs<GalleryListResult>(body, ::parseGalleryInfoList)
    }
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Can't parse gallery list", it)
    }

  val emptyResult = GalleryListResult(null, null, arrayListOf())
}

@Serializable
data class GalleryListResult(
  val prev: String?,
  val next: String?,
  val galleryInfoList: ArrayList<BaseGalleryInfo>,
)

/** Invokes the frozen, unversioned `ehviewer_rust` Gallery List rollback parser. */
private external fun parseGalleryInfoList(body: ByteBuffer, size: Int = body.limit()): Int

/** Invokes the V1 `eh_net::gallery::list` parser and returns an `ApiEnvelope<GalleryPage>`. */
private external fun parseGalleryInfoListV1(body: ByteBuffer, size: Int = body.limit()): Int
