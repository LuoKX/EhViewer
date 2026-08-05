package com.hippo.ehviewer.bridge

import com.ehviewer.core.database.model.QuickSearch
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 sanitized snapshot used by Rust to plan one Android-owned QuickSearch import. */
@Serializable
internal data class QuickSearchImportRequest(
  val version: Int,
  val currentNames: List<String>,
  val importedNames: List<String>,
  val currentCount: Int,
) {
  companion object {
    fun create(current: List<QuickSearch>, imported: List<QuickSearch>) =
      QuickSearchImportRequest(
        version = LIBRARY_RULE_API_VERSION,
        currentNames = current.map(QuickSearch::name),
        importedNames = imported.map(QuickSearch::name),
        currentCount = current.size,
      )
  }
}

/**
 * V1 source-aligned import decisions.
 *
 * A null position skips the corresponding imported row. A non-null position imports that row after
 * clearing its source Room ID.
 */
@Serializable internal data class QuickSearchImportPlan(val positions: List<Int?>)

internal typealias QuickSearchImportNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

/** Plans and adapts one QuickSearch import before the Android-owned Room insert. */
internal fun resolveQuickSearchImport(
  current: List<QuickSearch>,
  imported: List<QuickSearch>,
  nativeCall: QuickSearchImportNativeCall = ::nativePlanQuickSearchImportV1,
): List<QuickSearch> {
  val request = QuickSearchImportRequest.create(current, imported)
  val plan = planQuickSearchImport(request, nativeCall)
  return applyQuickSearchImportPlan(request, imported, plan)
}

/**
 * Invokes the strict Rust planner and validates that its result is safe to apply by source index.
 */
internal fun planQuickSearchImport(
  request: QuickSearchImportRequest,
  nativeCall: QuickSearchImportNativeCall = ::nativePlanQuickSearchImportV1,
): QuickSearchImportPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<QuickSearchImportPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "QuickSearch import plan",
      nativeCall = nativeCall,
    )
  require(envelope.version == LIBRARY_RULE_API_VERSION) {
    "Unsupported Library Rule API version"
  }
  require(envelope.module == LIBRARY_RULE_MODULE) { "Unexpected Library Rule module" }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing QuickSearch import payload" }
  require(plan.positions.size == request.importedNames.size) {
    "QuickSearch import plan size does not match its source"
  }

  var expectedPosition = request.currentCount
  plan.positions.forEach { position ->
    if (position != null) {
      require(position == expectedPosition) { "QuickSearch import positions are not contiguous" }
      // Kotlin/JVM Int addition intentionally wraps, matching Rust's `i32::wrapping_add` contract.
      expectedPosition += 1
    }
  }
  return plan
}

private fun applyQuickSearchImportPlan(
  request: QuickSearchImportRequest,
  imported: List<QuickSearch>,
  plan: QuickSearchImportPlan,
): List<QuickSearch> {
  require(imported.map(QuickSearch::name) == request.importedNames) {
    "QuickSearch import source changed after planning"
  }
  return buildList {
    imported.forEachIndexed { index, quickSearch ->
      plan.positions[index]?.let { position ->
        quickSearch.id = null
        quickSearch.position = position
        add(quickSearch)
      }
    }
  }
}

/** V1 JNI boundary owned by `eh_ffi::jvm::plan`. */
private external fun nativePlanQuickSearchImportV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int
