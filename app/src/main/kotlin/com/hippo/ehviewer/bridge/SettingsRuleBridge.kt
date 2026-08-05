package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable
data class SettingsRepairRequest(
  val version: Int,
  val chineseLocale: Boolean,
  val showTagTranslationsAbsent: Boolean,
  val downloadFilterMode: Int,
  val recentDownloadLabel: String?,
  val orientationMode: Int,
  val validOrientationModes: List<Int>,
) {
  companion object {
    fun create(
      chineseLocale: Boolean,
      showTagTranslationsAbsent: Boolean,
      downloadFilterMode: Int,
      recentDownloadLabel: String?,
      orientationMode: Int,
      validOrientationModes: List<Int>,
    ) =
      SettingsRepairRequest(
        version = SETTINGS_RULE_API_VERSION,
        chineseLocale = chineseLocale,
        showTagTranslationsAbsent = showTagTranslationsAbsent,
        downloadFilterMode = downloadFilterMode,
        recentDownloadLabel = recentDownloadLabel,
        orientationMode = orientationMode,
        validOrientationModes = validOrientationModes,
      )
  }
}

@Serializable
data class SettingsRepairPlan(
  val enableTagTranslations: Boolean,
  val resetRecentDownloadLabel: Boolean,
  val removeOrientationMode: Boolean,
)

internal fun resolveSettingsRepair(
  request: SettingsRepairRequest,
  rustPlan: (SettingsRepairRequest) -> SettingsRepairPlan = ::planSettingsRepairWithRust,
): SettingsRepairPlan = rustPlan(request)

private fun planSettingsRepairWithRust(request: SettingsRepairRequest) =
  invokeSettingsRule<SettingsRepairPlan>(
    rustCbor.encodeToByteArray(request),
    "settings repair",
    ::nativePlanSettingsRepairV1,
  )

@Serializable
data class ReadCacheSizeRequest(val version: Int, val configuredMib: Int) {
  companion object {
    fun create(configuredMib: Int) = ReadCacheSizeRequest(SETTINGS_RULE_API_VERSION, configuredMib)
  }
}

@Serializable data class ReadCacheSizePlan(val sizeMib: Int)

internal fun resolveReadCacheSize(
  request: ReadCacheSizeRequest,
  rustPlan: (ReadCacheSizeRequest) -> ReadCacheSizePlan = ::planReadCacheSizeWithRust,
): Int = rustPlan(request).sizeMib

private fun planReadCacheSizeWithRust(request: ReadCacheSizeRequest) =
  invokeSettingsRule<ReadCacheSizePlan>(
    rustCbor.encodeToByteArray(request),
    "read cache size",
    ::nativePlanReadCacheSizeV1,
  )

private inline fun <reified T> invokeSettingsRule(
  request: ByteArray,
  operation: String,
  noinline nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
): T {
  val envelope =
    invokeRustCbor<ApiEnvelope<T>>(
      request,
      operation = "Settings Rule $operation",
      nativeCall = nativeCall,
    )
  require(envelope.version == SETTINGS_RULE_API_VERSION) {
    "Unexpected Settings Rule API version ${envelope.version}"
  }
  require(envelope.module == SETTINGS_RULE_MODULE) {
    "Unexpected Settings Rule module ${envelope.module}"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload) { "Settings Rule $operation returned no payload" }
}

private external fun nativePlanSettingsRepairV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private external fun nativePlanReadCacheSizeV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private const val SETTINGS_RULE_API_VERSION = 1
private const val SETTINGS_RULE_MODULE = "settingsRule"
