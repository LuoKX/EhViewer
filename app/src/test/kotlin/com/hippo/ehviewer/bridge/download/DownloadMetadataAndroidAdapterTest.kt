package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadArtist
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.GalleryEntity
import com.hippo.ehviewer.spider.ComicInfo
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import okio.Path.Companion.toPath

class DownloadMetadataAndroidAdapterTest {
  @Test
  fun pinningDistinguishesNullFromEmptyTagsAndDeepCopiesMutableRelations() {
    val tags = mutableListOf("tag")
    val artists = mutableListOf(DownloadArtist(1, "artist"))
    val info = row(1, 4, tags, artists)
    val pinned = PinnedDownloadMetadata(info, "dir".toPath())
    assertEquals(DownloadMetadataSnapshot(4, false, false, true), pinned.snapshot)
    tags.clear()
    artists.clear()
    assertFalse(pinned.isCurrent(info))
    assertFalse(PinnedDownloadMetadata(row(2, 4, emptyList()), null).snapshot.tagsMissing)
    assertTrue(PinnedDownloadMetadata(row(2, 4, null), null).snapshot.tagsMissing)
    assertFalse(PinnedDownloadMetadata(row(2), null).snapshot.hasDirectory)
  }

  @Test
  fun observationPreservesComicPrecedenceMissingPencillerAndConditionalFallback() = runBlocking {
    val calls = mutableListOf<String>()
    val comic = ComicInfo(null, null, null, null, null, "ignored", -1, null, null, null, null, null)
    val present =
      observeDownloadMetadata(
        DownloadMetadataRead(0, true),
        {
          calls += "comic"
          comic
        },
        {
          calls += "spider"
          5
        },
      )
    assertEquals(listOf("comic"), calls)
    assertEquals(DownloadMetadataComicFacts(-1, null, null), present.comic)
    assertNull(present.spiderPages)
    calls.clear()
    val missing =
      observeDownloadMetadata(
        DownloadMetadataRead(0, true),
        {
          calls += "comic"
          null
        },
        {
          calls += "spider"
          -9
        },
      )
    assertEquals(listOf("comic", "spider"), calls)
    assertEquals(-9, missing.spiderPages)
    calls.clear()
    val tagsOnly =
      observeDownloadMetadata(
        DownloadMetadataRead(0, false),
        {
          calls += "comic"
          null
        },
        { fail("Unauthorized fallback") },
      )
    assertEquals(listOf("comic"), calls)
    assertNull(tagsOnly.comic)
    assertNull(tagsOnly.spiderPages)
  }

  @Test
  fun unavailableOrCorruptBothSidecarsProduceNoFactsAndNoPersistence() = runBlocking {
    val fact = observeDownloadMetadata(DownloadMetadataRead(0, true), { null }, { null })
    assertEquals(DownloadMetadataObservation(0, null, null), fact)
    val info = row(1)
    val result =
      applyDownloadMetadata(
        listOf(PinnedDownloadMetadata(info, null)),
        emptyList(),
        {
          info
        },
        { _, _ -> fail("No-op artists persisted") },
        { fail("No-op gallery persisted") },
      )
    assertEquals(DownloadMetadataApplyResult.APPLIED, result)
    assertEquals(0, info.pages)
  }

  @Test
  fun validBatchPreservesNullTagsDuplicateArtistsAndGalleryPersistenceIntent() = runBlocking {
    val rows =
      listOf(row(1, tags = listOf("old")), row(2, 5, emptyList()), row(3, tags = listOf("keep")))
    val pinned = rows.map { PinnedDownloadMetadata(it, "dir".toPath()) }
    val writes = mutableListOf<String>()
    val patches =
      listOf(
        DownloadMetadataPatch(0, 7, true, null, listOf("a", "a"), true, true),
        DownloadMetadataPatch(1, null, false, null, emptyList(), false, true),
        DownloadMetadataPatch(2, 0, false, null, null, true, false),
      )
    val result =
      applyDownloadMetadata(
        pinned,
        patches,
        { gid ->
          rows.firstOrNull {
            it.gid == gid
          }
        },
        { gid, artists ->
          writes += "artists:$gid"
          assertEquals(rows.first { it.gid == gid }.artistInfoList, artists)
        },
        { galleries ->
          writes += "gallery"
          assertEquals(listOf(rows[0].galleryInfo, rows[2].galleryInfo), galleries)
        },
      )
    assertEquals(DownloadMetadataApplyResult.APPLIED, result)
    assertEquals(listOf("artists:1", "artists:2", "gallery"), writes)
    assertEquals(7, rows[0].pages)
    assertNull(rows[0].simpleTags)
    assertEquals(listOf(DownloadArtist(1, "a"), DownloadArtist(1, "a")), rows[0].artistInfoList)
    assertEquals(5, rows[1].pages)
    assertEquals(emptyList(), rows[1].simpleTags)
    assertEquals(listOf("keep"), rows[2].simpleTags)
  }

  @Test
  fun mixedBatchSingleStaleSourceMeansZeroWritesForDeletionReplacementOrFieldChanges() =
    runBlocking {
      for (change in listOf("delete", "replace", "pages", "tags", "artists", "gid")) {
        val first = row(1)
        val second = row(2)
        val rows = mutableMapOf(1L to first, 2L to second)
        val pinned = listOf(first, second).map { PinnedDownloadMetadata(it, "dir".toPath()) }
        when (change) {
          "delete" -> rows.remove(2)
          "replace" -> rows[2] = row(2)
          "pages" -> second.pages = 4
          "tags" -> second.simpleTags = emptyList()
          "artists" -> second.artistInfoList = listOf(DownloadArtist(2, "new"))
          "gid" -> second.gid = 8
        }
        val patches =
          listOf(
              0,
              1,
            )
            .map { DownloadMetadataPatch(it, 7, true, null, listOf("a"), true, true) }
        val result =
          applyDownloadMetadata(
            pinned,
            patches,
            rows::get,
            { _, _ ->
              fail("Stale artists persisted: $change")
            },
            {
              fail("Stale gallery persisted: $change")
            },
          )
        assertEquals(DownloadMetadataApplyResult.STALE, result, change)
        assertEquals(0, first.pages, change)
        assertNull(first.simpleTags, change)
        assertTrue(first.artistInfoList.isEmpty(), change)
      }
    }

  @Test
  fun outOfRangeOrDuplicateSourceIndicesRejectBeforeMemoryAndRoomWrites() = runBlocking {
    val info = row(1)
    val pinned = listOf(PinnedDownloadMetadata(info, null))
    val patch = DownloadMetadataPatch(0, 7, true, null, null, true, false)
    for (patches in
      listOf(
        listOf(patch.copy(index = -1)),
        listOf(patch.copy(index = 1)),
        listOf(patch, patch),
      )) {
      assertFailsWith<IllegalArgumentException> {
        applyDownloadMetadata(
          pinned,
          patches,
          {
            info
          },
          { _, _ -> fail("Artists written") },
          { fail("Gallery written") },
        )
      }
      assertEquals(0, info.pages)
    }
  }

  @Test
  fun cancellationBeforeApplyOrDuringObservationAlwaysPropagatesWithoutWrites() = runBlocking {
    val info = row(1)
    val job = Job().apply { cancel() }
    assertFailsWith<CancellationException> {
      withContext(job) {
        applyDownloadMetadata(
          listOf(PinnedDownloadMetadata(info, null)),
          listOf(DownloadMetadataPatch(0, 7, true, null, null, true, false)),
          {
            info
          },
          { _, _ -> fail("Artists written") },
          { fail("Gallery written") },
        )
      }
    }
    val failure = CancellationException("fixture")
    assertSame(
      failure,
      assertFailsWith<CancellationException> {
        observeDownloadMetadata(
          DownloadMetadataRead(0, true),
          {
            throw failure
          },
          { fail("Cancellation fell back") },
        )
      },
    )
    assertSame(
      failure,
      assertFailsWith<CancellationException> {
        observeDownloadMetadata(DownloadMetadataRead(0, true), { null }, { throw failure })
      },
    )
    assertEquals(0, info.pages)
  }

  @Test
  fun persistenceFailureSurfacesWithoutReplayOrFallback() = runBlocking {
    val info = row(1)
    var writes = 0
    val failure = CancellationException("fixture")
    assertSame(
      failure,
      assertFailsWith<CancellationException> {
        applyDownloadMetadata(
          listOf(PinnedDownloadMetadata(info, null)),
          listOf(DownloadMetadataPatch(0, 0, false, null, listOf("a"), true, true)),
          {
            info
          },
          { _, _ ->
            writes++
            throw failure
          },
          { fail("Cancelled persistence continued") },
        )
      },
    )
    assertEquals(1, writes)
  }

  private fun row(
    gid: Long,
    pages: Int = 0,
    tags: List<String>? = null,
    artists: List<DownloadArtist> = emptyList(),
  ) =
    DownloadInfo(
      GalleryEntity(gid, "token", "title", null, null, 0, null, null, 0f, tags, pages, null, -1),
      "dir",
      artists,
    )
}
