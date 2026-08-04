package com.hippo.ehviewer.bridge

import com.hippo.ehviewer.Settings

/** The mutually exclusive implementation selected for one operation. */
internal enum class CoreBackend {
  LEGACY,
  RUST,
}

internal fun selectCoreBackend(useRustCore: Boolean): CoreBackend =
  if (useRustCore) CoreBackend.RUST else CoreBackend.LEGACY

/** Captures the global setting once; callers retain the returned value for the whole operation. */
internal object CoreBackendSelector {
  fun snapshot(): CoreBackend = selectCoreBackend(Settings.useRustCore.value)
}
