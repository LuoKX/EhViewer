package com.hippo.ehviewer.jni

import android.os.Build
import java.nio.ByteBuffer

/** True only for the platform releases affected by Android's short-GIF-delay decoder behavior. */
internal fun shouldUseGifWorkaround(sdkInt: Int) =
  sdkInt in Build.VERSION_CODES.P until Build.VERSION_CODES.UPSIDE_DOWN_CAKE

/**
 * Returns a new rewritten GIF snapshot, or null for a non-GIF or a descriptor that cannot be
 * classified without moving its position.
 */
internal fun prepareGifSourceOwned(
  fd: Int,
  nativeCall: (Int) -> ByteArray? = ::prepareGifSource,
): ByteArray? = nativeCall(fd)

private external fun prepareGifSource(fd: Int): ByteArray?

/** Rewrites a caller-owned direct GIF buffer synchronously in place. */
external fun rewriteGifSource(buffer: ByteBuffer)
