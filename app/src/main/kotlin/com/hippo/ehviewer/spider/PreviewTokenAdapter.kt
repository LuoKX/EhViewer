package com.hippo.ehviewer.spider

import com.hippo.ehviewer.bridge.reader.PreviewToken
import com.hippo.ehviewer.bridge.reader.PreviewTokenKind
import com.hippo.ehviewer.bridge.reader.PreviewTokenRequest
import com.hippo.ehviewer.bridge.reader.planPreviewTokens
import java.nio.ByteBuffer

/** Apply only validated deltas to the current synchronized map, never a stale snapshot. */
internal fun SpiderInfo.applyPreviews(
  previews: List<PreviewToken>,
  previewIndex: Int,
  nativeCall: ((ByteBuffer, Int) -> ByteArray)? = null,
) {
  val facts = previews.toList()
  val request =
    PreviewTokenRequest(
      kind = PreviewTokenKind.APPLY,
      previewIndex = previewIndex,
      previews = facts,
    )
  val plan =
    if (nativeCall == null) {
      planPreviewTokens(request)
    } else {
      planPreviewTokens(request, nativeCall)
    }
  previewPerPage = checkNotNull(plan.previewPerPage)
  checkNotNull(plan.updates).forEach { index ->
    val row = facts[index]
    pTokenMap[row.position] = row.pToken
  }
}

/** Selection happens before HTTP; coroutine/retry and lock ownership remain with caller. */
internal fun SpiderInfo.previewIndex(
  index: Int,
  nativeCall: ((ByteBuffer, Int) -> ByteArray)? = null,
): Int {
  val request =
    PreviewTokenRequest(
      kind = PreviewTokenKind.LOOKUP,
      index = index,
      previewPerPage = previewPerPage,
    )
  val plan =
    if (nativeCall == null) {
      planPreviewTokens(request)
    } else {
      planPreviewTokens(request, nativeCall)
    }
  return checkNotNull(plan.previewIndex)
}
