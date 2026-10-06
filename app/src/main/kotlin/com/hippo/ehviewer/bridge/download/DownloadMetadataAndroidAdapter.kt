package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadArtist
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.GalleryEntity
import com.hippo.ehviewer.spider.ComicInfo
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import okio.Path

/** Deep field copies remain Android-owned; JNI receives only missing-field facts. */
internal class PinnedDownloadMetadata(
  val info: DownloadInfo,
  val directory: Path?,
) {
  val gid = info.gid
  private val pages = info.pages
  private val tags = info.simpleTags?.toList()
  private val artists = info.artistInfoList.map { it.copy() }
  val snapshot = DownloadMetadataSnapshot(pages, tags == null, artists.isEmpty(), directory != null)

  fun isCurrent(current: DownloadInfo?) =
    current === info &&
      info.gid == gid &&
      info.pages == pages &&
      info.simpleTags == tags &&
      info.artistInfoList == artists
}

/** Execute only Rust-authorized read order. Existing codecs own corrupt/unavailable semantics. */
internal suspend fun observeDownloadMetadata(
  read: DownloadMetadataRead,
  readComic: suspend () -> ComicInfo?,
  readSpiderPages: suspend () -> Int?,
): DownloadMetadataObservation {
  currentCoroutineContext().ensureActive()
  val comic = readComic()
  val spiderPages = if (comic == null && read.spiderIfComicMissing) readSpiderPages() else null
  currentCoroutineContext().ensureActive()
  return DownloadMetadataObservation(
    read.index,
    comic?.let {
      DownloadMetadataComicFacts(it.pageCount, it.simpleTags?.toList(), it.penciller?.toList())
    },
    spiderPages,
  )
}

internal enum class DownloadMetadataApplyResult {
  APPLIED,
  STALE,
}

/**
 * Caller holds sortMutex. Preflight all patches before any row/Room write; stale is whole-batch
 * no-op.
 */
internal suspend fun applyDownloadMetadata(
  pinned: List<PinnedDownloadMetadata>,
  patches: List<DownloadMetadataPatch>,
  current: (Long) -> DownloadInfo?,
  persistArtists: suspend (Long, List<DownloadArtist>) -> Unit,
  persistGalleries: suspend (List<GalleryEntity>) -> Unit,
): DownloadMetadataApplyResult {
  currentCoroutineContext().ensureActive()
  require(
    patches.all { it.index in pinned.indices } &&
      patches.map { it.index }.distinct().size == patches.size
  ) {
    "Invalid download metadata source indices"
  }
  if (
    patches.any {
      !pinned[it.index].isCurrent(current(pinned[it.index].gid))
    }
  ) {
    return DownloadMetadataApplyResult.STALE
  }
  patches.forEach { patch ->
    val info = pinned[patch.index].info
    patch.pages?.let { info.pages = it }
    if (patch.updateTags) info.simpleTags = patch.simpleTags
    patch.artists?.let { info.artistInfoList = DownloadArtist.from(info.gid, it) }
  }
  val galleries = buildList {
    patches.forEach { patch ->
      val info = pinned[patch.index].info
      if (patch.persistArtists) persistArtists(info.gid, info.artistInfoList)
      if (patch.persistGallery) add(info.galleryInfo)
    }
  }
  if (galleries.isNotEmpty()) persistGalleries(galleries)
  return DownloadMetadataApplyResult.APPLIED
}
