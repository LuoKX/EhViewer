@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

@Serializable
internal data class ImportDownload(
  val gid: Long,
  val state: Int,
)

/** Raw Java String equality, including lone surrogates; old imported positions ignored by Rust. */
@Serializable
internal data class ImportLabel(
  @ByteString val labelUtf16: ByteArray,
  val position: Int,
)

@Serializable
internal data class BackupImportRequest(
  val version: Int,
  val existingGids: List<Long>,
  val downloads: List<ImportDownload>,
  val currentLabels: List<ImportLabel>,
  val labels: List<ImportLabel>,
)

@Serializable
internal data class ImportDownloadAddition(
  val sourceIndex: Int,
  val gid: Long,
  val state: Int,
)

@Serializable
internal data class ImportLabelAddition(
  val sourceIndex: Int,
  val position: Int,
)

@Serializable
internal data class BackupImportPlan(
  val downloads: List<ImportDownloadAddition>,
  val labels: List<ImportLabelAddition>,
)

internal fun importLabel(
  name: String,
  position: Int,
) = ImportLabel(name.toUtf16LeBytes(), position)

/** One bounded call; validate whole response before caller copies or writes any selected rows. */
internal fun planBackupImport(
  request: BackupImportRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanBackupImportV1,
): BackupImportPlan {
  require(
    request.version == 1 &&
      request.existingGids.size.toLong() +
        request.downloads.size +
        request.currentLabels.size +
        request.labels.size <= 100_000
  ) {
    "Invalid backup import input"
  }
  val names = request.currentLabels.asSequence() + request.labels.asSequence()
  require(
    names.all { it.labelUtf16.size <= 8192 && it.labelUtf16.size % 2 == 0 } &&
      names.sumOf { it.labelUtf16.size.toLong() } <= 4 * 1024 * 1024
  ) {
    "Invalid backup import input"
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size in 1..8 * 1024 * 1024) { "Invalid backup import request size" }
  val input =
    ByteBuffer.allocateDirect(encoded.size).apply {
      put(encoded)
      flip()
    }
  // Plans contain indices and integers, never label names or Room entities.
  val capacity = 512 + (request.downloads.size + request.labels.size) * 64
  val output = ByteBuffer.allocateDirect(capacity)
  val size = nativeCall(input, encoded.size, output)
  require(size in 1..capacity) { "Invalid backup import response size" }
  val bytes =
    ByteArray(size).also {
      output.position(0)
      output.get(it)
    }
  val envelope = rustCbor.decodeFromByteArray<ApiEnvelope<BackupImportPlan>>(bytes)
  require(envelope.version == 1 && envelope.module == "backupImport") {
    "Invalid backup import envelope"
  }
  envelope.error?.let {
    // Preserve repaired helper's public overflow exception without recomputing merge policy.
    if (
      it.kind == RustErrorKind.ERROR && it.message == "Download label positions exceed Int range"
    ) {
      throw IllegalArgumentException("Download label positions exceed Int range")
    }
    throw RustFailureException(it)
  }
  val plan = checkNotNull(envelope.payload) { "Missing backup import payload" }
  validateBackupImportPlan(request, plan)
  return plan
}

private fun validateBackupImportPlan(
  request: BackupImportRequest,
  plan: BackupImportPlan,
) {
  val existing = request.existingGids.toHashSet()
  val firstGids = HashMap<Long, Int>()
  request.downloads.forEachIndexed { index, row -> firstGids.putIfAbsent(row.gid, index) }
  require(
    plan.downloads.zipWithNext().all { (left, right) -> left.sourceIndex < right.sourceIndex } &&
      plan.downloads.map { it.gid }.distinct().size == plan.downloads.size &&
      plan.downloads.all { row ->
        val source = request.downloads.getOrNull(row.sourceIndex)
        source != null &&
          source.gid == row.gid &&
          row.gid !in existing &&
          firstGids[row.gid] == row.sourceIndex &&
          row.state == if (source.state == 1 || source.state == 2) 0 else source.state
      }
  ) {
    "Invalid backup download plan"
  }

  // Decode only for equality: Char constructor preserves every UTF-16 code unit.
  fun ImportLabel.name(): String =
    CharArray(labelUtf16.size / 2) { index ->
        ((labelUtf16[index * 2].toInt() and 255) or
            ((labelUtf16[index * 2 + 1].toInt() and 255) shl 8))
          .toChar()
      }
      .concatToString()
  val currentNames = request.currentLabels.mapTo(HashSet()) { it.name() }
  val sourceNames = request.labels.map { it.name() }
  val firstNames = HashMap<String, Int>()
  sourceNames.forEachIndexed { index, name -> firstNames.putIfAbsent(name, index) }
  val maximum = request.currentLabels.maxOfOrNull { it.position } ?: -1
  require(
    plan.labels.zipWithNext().all { (left, right) -> left.sourceIndex < right.sourceIndex } &&
      plan.labels.map { it.sourceIndex }.distinct().size == plan.labels.size &&
      plan.labels.withIndex().all { (offset, row) ->
        val name = sourceNames.getOrNull(row.sourceIndex)
        name != null &&
          name !in currentNames &&
          firstNames[name] == row.sourceIndex &&
          row.position.toLong() == maxOf(maximum.toLong() + 1, 0) + offset &&
          row.position >= 0
      }
  ) {
    "Invalid backup label plan"
  }
}

private external fun nativePlanBackupImportV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
