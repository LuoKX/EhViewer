@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.dao.RecoveryMergePolicy
import com.ehviewer.core.database.model.DownloadRecoveryCandidate
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.database.model.GalleryEntity
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class RecoveryPersistenceKind {
  @SerialName("formal") FORMAL,
  @SerialName("artists") ARTISTS,
  @SerialName("stage") STAGE,
  @SerialName("failure") FAILURE,
  @SerialName("finish") FINISH,
  @SerialName("pending") PENDING,
}

@Serializable internal data class RecoveryRawText(@ByteString val value: ByteArray)

@Serializable
internal data class RecoveryPersistenceRequest(
  @EncodeDefault val version: Int = 1,
  val kind: RecoveryPersistenceKind,
  val exists: Boolean? = null,
  val blank: List<Boolean>? = null,
  val category: Int? = null,
  val ratingBits: Long? = null,
  val hasTags: Boolean? = null,
  val pages: Int? = null,
  val names: List<RecoveryRawText>? = null,
  val whitespace: List<Int>? = null,
  val action: RecoveryPersistenceAction? = null,
  val status: RecoveryStatus? = null,
  val issues: List<String>? = null,
  val pathAvailable: Boolean? = null,
  val present: List<Boolean>? = null,
  @ByteString val issuesText: ByteArray? = null,
  val limits: List<Boolean>? = null,
  val sameRoot: List<Boolean>? = null,
  val missing: List<Boolean>? = null,
)

@Serializable
internal data class RecoveryPersistencePlan(
  val flags: List<Boolean>,
  val spans: List<List<Int>>,
  @ByteString val text: ByteArray?,
  val status: String?,
  val count: Int?,
)

internal fun planRecoveryPersistence(
  request: RecoveryPersistenceRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeRecoveryPersistenceV1,
): RecoveryPersistencePlan {
  require((request.names?.size ?: 0) <= 100_000 && (request.sameRoot?.size ?: 0) <= 100_000)
  val bytes = rustCbor.encodeToByteArray(request)
  require(bytes.size <= 16 * 1024 * 1024) { "Recovery persistence request exceeds limit" }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<RecoveryPersistencePlan>>(
      bytes,
      "Recovery persistence",
      maxResponseSize = 16 * 1024 * 1024,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "recoveryPersistence")
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload).also { p ->
    val hasText =
      request.kind == RecoveryPersistenceKind.FAILURE ||
        request.kind == RecoveryPersistenceKind.STAGE &&
          request.action != RecoveryPersistenceAction.KEEP_EXISTING
    require((p.text != null) == hasText && (p.status != null) == hasText)
    p.text?.let { require(it.size % 2 == 0) }
    p.status?.let { DownloadRecoveryStatus.valueOf(it) }
    require((p.count != null) == (request.kind == RecoveryPersistenceKind.PENDING))
    if (request.kind != RecoveryPersistenceKind.ARTISTS) require(p.spans.isEmpty())
    when (request.kind) {
      RecoveryPersistenceKind.FORMAL ->
        require(
          p.flags.size == 11 &&
            p.flags[0] == request.exists &&
            (request.exists == true || p.flags.none { it })
        )
      RecoveryPersistenceKind.ARTISTS -> {
        require(p.flags.isEmpty())
        val names = checkNotNull(request.names)
        require(
          p.spans.all { span ->
            span.size == 3 &&
              span[0] in names.indices &&
              span[1] >= 0 &&
              span[1] < span[2] &&
              span[2] <= names[span[0]].value.size / 2
          }
        )
        require(p.spans.zipWithNext().all { (a, b) -> a[0] < b[0] })
      }
      RecoveryPersistenceKind.STAGE ->
        require(
          if (request.action == RecoveryPersistenceAction.KEEP_EXISTING) p.flags == listOf(true)
          else p.flags.size == 7 && !p.flags[0] && p.flags.drop(1) == request.present
        )
      RecoveryPersistenceKind.FAILURE -> require(p.flags.isEmpty() && p.status == "PATH_CONFLICT")
      RecoveryPersistenceKind.FINISH -> require(p.flags.size == 2 && p.flags[0] != p.flags[1])
      RecoveryPersistenceKind.PENDING ->
        require(
          p.flags.isEmpty() && checkNotNull(p.count) in 0..checkNotNull(request.sameRoot).size
        )
    }
  }
}

internal fun recoveryWhitespace(texts: List<String>) =
  texts
    .asSequence()
    .flatMap { it.asSequence() }
    .filter(Char::isWhitespace)
    .map(Char::code)
    .distinct()
    .toList()

internal fun ByteArray.recoveryString(): String {
  require(size % 2 == 0)
  return CharArray(size / 2) { i ->
      ((this[i * 2].toInt() and 255) or ((this[i * 2 + 1].toInt() and 255) shl 8)).toChar()
    }
    .concatToString()
}

/** No JNI dependency enters core/data; callbacks execute within the generated Room transaction. */
internal object RustRecoveryMergePolicy : RecoveryMergePolicy {
  override fun mergeGallery(existing: GalleryEntity?, incoming: GalleryEntity): GalleryEntity {
    val flags =
      planRecoveryPersistence(
          RecoveryPersistenceRequest(
            kind = RecoveryPersistenceKind.FORMAL,
            exists = existing != null,
            blank =
              listOf(
                  existing?.title,
                  existing?.titleJpn,
                  existing?.thumbKey,
                  existing?.posted,
                  existing?.uploader,
                  existing?.simpleLanguage,
                )
                .map { it.isNullOrBlank() },
            category = existing?.category ?: 0,
            ratingBits = (existing?.rating ?: 0f).toRawBits().toLong() and 0xffffffffL,
            hasTags = !existing?.simpleTags.isNullOrEmpty(),
            pages = existing?.pages ?: 0,
          )
        )
        .flags
    if (!flags[0]) return incoming
    val old = checkNotNull(existing)
    return GalleryEntity(
      gid = old.gid,
      token = old.token,
      title = if (flags[1]) old.title else incoming.title,
      titleJpn = if (flags[2]) old.titleJpn else incoming.titleJpn,
      thumbKey = if (flags[3]) old.thumbKey else incoming.thumbKey,
      category = if (flags[4]) old.category else incoming.category,
      posted = if (flags[5]) old.posted else incoming.posted,
      uploader = if (flags[6]) old.uploader else incoming.uploader,
      rating = if (flags[7]) old.rating else incoming.rating,
      simpleTags = if (flags[8]) old.simpleTags else incoming.simpleTags,
      pages = if (flags[9]) old.pages else incoming.pages,
      simpleLanguage = if (flags[10]) old.simpleLanguage else incoming.simpleLanguage,
      favoriteSlot = old.favoriteSlot,
    )
  }

  override fun mergeArtists(existing: List<String>, incoming: List<String>): List<String> {
    val names = existing + incoming
    require(names.size <= 100_000 && names.sumOf { it.length.toLong() * 2 } <= 8 * 1024 * 1024)
    val plan =
      planRecoveryPersistence(
        RecoveryPersistenceRequest(
          kind = RecoveryPersistenceKind.ARTISTS,
          names = names.map { RecoveryRawText(it.toUtf16LeBytes()) },
          whitespace = recoveryWhitespace(names),
        )
      )
    return plan.spans.map { (index, start, end) -> names[index].substring(start, end) }
  }
}

/** Apply selectors to the observed rows; upsertBySourceKey still rechecks identity/time in Room. */
internal fun applyRecoveryStage(
  incoming: DownloadRecoveryCandidate,
  old: DownloadRecoveryCandidate?,
  plan: RecoveryPersistencePlan,
): DownloadRecoveryCandidate {
  val fields = plan.flags
  require(fields.size == 7 && !fields[0])
  return incoming.copy(
    candidateId = old?.candidateId ?: incoming.candidateId,
    gid = if (fields[1]) incoming.gid else old?.gid,
    token = if (fields[2]) incoming.token else old?.token,
    pageToken = if (fields[3]) incoming.pageToken else old?.pageToken,
    pageIndex = if (fields[4]) incoming.pageIndex else old?.pageIndex,
    title = if (fields[5]) incoming.title else old?.title,
    titleJpn = old?.titleJpn,
    declaredPages = if (fields[6]) incoming.declaredPages else old?.declaredPages,
    issues = checkNotNull(plan.text).recoveryString(),
    status = DownloadRecoveryStatus.valueOf(checkNotNull(plan.status)),
    createdAt = old?.createdAt ?: incoming.createdAt,
  )
}

internal fun recoveryPromotionFailure(
  row: DownloadRecoveryCandidate,
  now: () -> Long,
): DownloadRecoveryCandidate {
  val p =
    planRecoveryPersistence(
      RecoveryPersistenceRequest(
        kind = RecoveryPersistenceKind.FAILURE,
        issuesText = row.issues.toUtf16LeBytes(),
        whitespace = recoveryWhitespace(listOf(row.issues)),
      )
    )
  return row.copy(
    status = DownloadRecoveryStatus.valueOf(checkNotNull(p.status)),
    issues = checkNotNull(p.text).recoveryString(),
    updatedAt = now(),
  )
}

internal external fun nativeRecoveryPersistenceV1(request: ByteBuffer, size: Int): ByteArray
