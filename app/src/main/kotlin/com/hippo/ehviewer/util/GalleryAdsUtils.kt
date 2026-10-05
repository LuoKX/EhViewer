package com.hippo.ehviewer.util

import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.reader.ReaderPolicyKind
import com.hippo.ehviewer.bridge.reader.ReaderPolicyRequest
import com.hippo.ehviewer.bridge.reader.planReaderPolicy
import okio.Path.Companion.toOkioPath
import splitties.init.appCtx

fun detectAds(index: Int, size: Int): Boolean =
  checkNotNull(
    planReaderPolicy(
        ReaderPolicyRequest(
          kind = ReaderPolicyKind.ADS,
          index = index,
          pages = size,
          enabled = Settings.stripExtraneousAds.value,
        )
      )
      .detect
  )

val GalleryInfo.hasAds
  get() =
    checkNotNull(
      planReaderPolicy(
          ReaderPolicyRequest(
            kind = ReaderPolicyKind.ADS,
            enabled = false,
            tags = simpleTags?.toList(),
          )
        )
        .hasAds
    )

val AdsPlaceholderFile = appCtx.filesDir.toOkioPath() / "AdsPlaceholder"
