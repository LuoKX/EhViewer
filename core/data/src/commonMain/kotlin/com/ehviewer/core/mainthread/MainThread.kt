package com.ehviewer.core.mainthread

internal expect val isMainThread: Boolean

fun checkNotMainThread() {
  check(!isMainThread) { "Must not be called from the main thread" }
}
