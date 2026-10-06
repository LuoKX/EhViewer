package com.hippo.ehviewer.spider

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.archive.*
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import java.io.IOException
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.decodeFromByteArray

class ComicInfoPolicyExecutorTest {
  @Test
  fun knownPositiveExportCountNeverReadsXml() {
    var calls = 0
    val result =
      observeComicInfoPolicy(
        ComicInfoPolicyRequest.resolveExport(5, false),
        readMetadata = { fail("Known count read XML") },
        planner = {
          calls++
          assertEquals(ComicInfoMetadataState.UNREAD, it.metadataState)
          fromNative(it, "resolve_known")
        },
      )
    assertEquals(1, calls)
    assertEquals(5, result.plan.pageCount)
    assertNull(result.current)
  }

  @Test
  fun preparationReadsOnceAndPreservesOpaqueDecodedObject() {
    val current = info(1)
    var calls = 0
    var reads = 0
    val result =
      observeComicInfoPolicy(
        ComicInfoPolicyRequest.prepare(2, false, true),
        readMetadata = {
          reads++
          current
        },
        planner = { request ->
          calls++
          if (calls == 1) {
            assertEquals(ComicInfoMetadataState.UNREAD, request.metadataState)
            fromNative(request, "read")
          } else {
            assertEquals(ComicInfoMetadataState.PRESENT, request.metadataState)
            assertEquals(1, request.metadataPages)
            fromNative(request, "rewrite")
          }
        },
      )
    assertEquals(2, calls)
    assertEquals(1, reads)
    assertSame(current, result.current)
    assertEquals(ComicInfoAction.REWRITE, result.plan.action)
  }

  @Test
  fun missingMetadataCreatesAfterExactlyOneRequestedObservation() {
    var calls = 0
    var reads = 0
    val result =
      observeComicInfoPolicy(
        ComicInfoPolicyRequest.prepare(2, false, true),
        readMetadata = {
          reads++
          null
        },
        planner = { request ->
          calls++
          if (calls == 1) fromNative(request, "read")
          else {
            assertEquals(ComicInfoMetadataState.MISSING, request.metadataState)
            assertNull(request.metadataPages)
            fromNative(request, "create")
          }
        },
      )
    assertEquals(2, calls)
    assertEquals(1, reads)
    assertEquals(ComicInfoFetchMode.CONTINUE, result.plan.fetchMode)
  }

  @Test
  fun validationFailureHappensBeforeFilesystemObservation() {
    val failure = IllegalArgumentException("Archive expected page count must be positive")
    val actual =
      assertFailsWith<IllegalArgumentException> {
        observeComicInfoPolicy(
          ComicInfoPolicyRequest.prepare(0, false, true),
          readMetadata = { fail("Invalid page count read XML") },
          planner = { throw failure },
        )
      }
    assertSame(failure, actual)
  }

  @Test
  fun filesystemObservationFailureIsNotTurnedIntoMissingOrRetried() {
    val failure = IOException("fixture")
    var calls = 0
    assertSame(
      failure,
      assertFailsWith<IOException> {
        observeComicInfoPolicy(
          ComicInfoPolicyRequest.prepare(2, false, true),
          readMetadata = { throw failure },
          planner = {
            calls++
            fromNative(it, "read")
          },
        )
      },
    )
    assertEquals(1, calls)
  }

  @Test
  fun unresolvedPlansNeverLoopOrApplyEffects() = runBlocking {
    var calls = 0
    var reads = 0
    assertFailsWith<IllegalStateException> {
      observeComicInfoPolicy(
        ComicInfoPolicyRequest.prepare(2, false, true),
        readMetadata = {
          reads++
          null
        },
        planner = {
          calls++
          plan("read")
        },
      )
    }
    assertEquals(2, calls)
    assertEquals(1, reads)
    val effects = Effects()
    assertFailsWith<IllegalStateException> { effects.execute(plan("read")) }
    assertTrue(effects.events.isEmpty())
  }

  @Test
  fun archiveFetchFailureLogsThenPinsDownloadedCountBeforeCreation() = runBlocking {
    val effects =
      Effects().apply {
        fetchPages = 77
        fetchFailure = IOException("fixture")
      }
    effects.execute(plan("create"))
    assertEquals(listOf("fetch", "log", "pages:2", "create:2", "artists"), effects.events)
    assertEquals(2, effects.galleryPages)
    assertSame(effects.fetchFailure, effects.logged.single())
  }

  @Test
  fun cancellationAndNonExceptionFailuresDoNotContinueArchivePreparation() = runBlocking {
    for (failure in listOf(CancellationException("fixture"), LinkageError("fixture"))) {
      val effects = Effects().apply { fetchFailure = failure }
      try {
        effects.execute(plan("create"))
        fail("Fetch failure hidden")
      } catch (actual: Throwable) {
        assertSame(failure, actual)
      }
      assertEquals(listOf("fetch"), effects.events)
      assertEquals(1, effects.galleryPages)
    }
  }

  @Test
  fun directRefreshFailuresPropagateAndSuccessfulFetchedCountIsNotOverwritten() = runBlocking {
    val failure = IOException("fixture")
    val failed = Effects().apply { fetchFailure = failure }
    assertSame(failure, assertFailsWith<IOException> { failed.execute(plan("write")) })
    assertEquals(listOf("fetch"), failed.events)
    val successful = Effects().apply { fetchPages = 77 }
    successful.execute(plan("write"))
    assertEquals(listOf("fetch", "create:77", "artists"), successful.events)
    assertEquals(77, successful.galleryPages)
  }

  @Test
  fun rewriteOnlyChangesPageCountAndKeepResolveSkipAvoidWrites() = runBlocking {
    val current = info(1)
    val rewritten = Effects(current)
    rewritten.execute(plan("rewrite"))
    assertEquals(current.copy(pageCount = 2), rewritten.rewritten)
    assertEquals(listOf("pages:2", "rewrite:2"), rewritten.events)
    for ((name, expected) in listOf("keep" to 2, "resolve_metadata" to 7)) {
      val effects = Effects()
      effects.execute(plan(name))
      assertEquals(listOf("pages:$expected"), effects.events)
    }
    val skipped = Effects()
    skipped.execute(plan("skip"))
    assertTrue(skipped.events.isEmpty())
    val detailed = Effects()
    detailed.execute(plan("detailed"))
    assertEquals(listOf("pages:2", "create:2", "artists"), detailed.events)
  }

  @Test
  fun fileWriteFailureStaysObservableAndCannotReachArtistPersistence() = runBlocking {
    val failure = IOException("write fixture")
    val effects = Effects().apply { createFailure = failure }
    assertSame(failure, assertFailsWith<IOException> { effects.execute(plan("detailed")) })
    assertEquals(listOf("pages:2", "create:2"), effects.events)
    assertEquals(2, effects.galleryPages)
  }

  private class Effects(private val current: ComicInfo? = null) {
    val events = mutableListOf<String>()
    val logged = mutableListOf<Exception>()
    var galleryPages = 1
    var fetchPages: Int? = null
    var fetchFailure: Throwable? = null
    var createFailure: Throwable? = null
    var rewritten: ComicInfo? = null

    suspend fun execute(plan: ComicInfoPolicyPlan) {
      executeComicInfoPolicy(
        plan,
        fetchMetadata = {
          events += "fetch"
          fetchPages?.let { galleryPages = it }
          fetchFailure?.let { throw it }
        },
        logFetchFailure = {
          events += "log"
          logged += it
        },
        setGalleryPages = {
          galleryPages = it
          events += "pages:$it"
        },
        create = {
          events += "create:$galleryPages"
          createFailure?.let { throw it }
          events += "artists"
        },
        rewrite = {
          rewritten = checkNotNull(current).copy(pageCount = it)
          events += "rewrite:$it"
        },
      )
    }
  }

  private fun info(pages: Int) =
    ComicInfo(
      series = "stored",
      alternateSeries = "alt",
      writer = listOf("group"),
      penciller = listOf("artist"),
      genre = listOf("genre"),
      web = "stored-web",
      pageCount = pages,
      languageISO = "en",
      characters = listOf("character"),
      teams = listOf("parody"),
      communityRating = "4.5",
      simpleTags = listOf("stored-tag"),
    )

  private fun golden(name: String) =
    File("src/main/rust/crates/domain/test_data/comic_info_policy_$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun plan(name: String) =
    rustCbor
      .decodeFromByteArray<ApiEnvelope<ComicInfoPolicyPlan>>(golden("${name}_response"))
      .payload!!

  private fun fromNative(request: ComicInfoPolicyRequest, name: String) =
    planComicInfoPolicy(request) { _, _, output ->
      val bytes = golden("${name}_response")
      output.put(bytes)
      bytes.size
    }
}
