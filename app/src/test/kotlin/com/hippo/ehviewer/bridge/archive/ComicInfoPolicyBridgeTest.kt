package com.hippo.ehviewer.bridge.archive

import com.hippo.ehviewer.bridge.*
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class ComicInfoPolicyBridgeTest {
  @Test
  fun allSharedGoldenActionsRouteOnceWithExactRequestBytes() {
    for (name in
      listOf(
        "read",
        "create",
        "detailed",
        "rewrite",
        "keep",
        "read_export",
        "resolve_known",
        "resolve_metadata",
        "write",
        "skip",
      )) {
      val bytes = golden("${name}_request")
      val request = rustCbor.decodeFromByteArray<ComicInfoPolicyRequest>(bytes)
      val expected =
        rustCbor
          .decodeFromByteArray<ApiEnvelope<ComicInfoPolicyPlan>>(golden("${name}_response"))
          .payload!!
      var calls = 0
      val actual =
        planComicInfoPolicy(request) { input, size, output ->
          calls++
          assertContentEquals(bytes, ByteArray(size).also { input.duplicate().get(it) })
          respond(output, golden("${name}_response"))
        }
      assertEquals(expected, actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun malformedObservationsNeverEnterJni() {
    val request = prepare()
    for (wrong in
      listOf(
        request.copy(version = 2),
        request.copy(expectedPages = null),
        request.copy(metadataState = ComicInfoMetadataState.PRESENT),
        request.copy(metadataPages = 1),
        request.copy(metadataState = ComicInfoMetadataState.MISSING, metadataPages = 0),
        write().copy(expectedPages = 2),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planComicInfoPolicy(wrong) { _, _, _ -> fail("Invalid request entered JNI") }
      }
    }
  }

  @Test
  fun legacyPageAndDirectoryErrorsKeepExactExceptionContract() {
    for ((name, message) in
      listOf(
        "invalid_pages" to "Archive expected page count must be positive",
        "missing_dir" to "Required value was null.",
        "export_missing" to "Archive expected page count must be positive",
      )) {
      var calls = 0
      val request = rustCbor.decodeFromByteArray<ComicInfoPolicyRequest>(golden("${name}_request"))
      val bytes = golden("${name}_response")
      val error =
        assertFailsWith<IllegalArgumentException> {
          planComicInfoPolicy(request) { _, _, output ->
            calls++
            respond(output, bytes)
          }
        }
      assertEquals(message, error.message)
      assertEquals(1, calls)
    }
  }

  @Test
  fun otherRustAndLinkageFailuresRemainObservableWithoutRetry() {
    assertFailsWith<RustFailureException> {
      planComicInfoPolicy(
        prepare(),
        envelope(
          ApiEnvelope(1, "comicInfoPolicy", error = RustError(RustErrorKind.ERROR, "fixture"))
        ),
      )
    }
    assertFailsWith<RustFailureException> {
      planComicInfoPolicy(
        prepare(),
        envelope(
          ApiEnvelope(
            1,
            "comicInfoPolicy",
            error =
              RustError(
                RustErrorKind.GALLERY_UNAVAILABLE,
                "Archive expected page count must be positive",
              ),
          )
        ),
      )
    }
    for (failure in listOf(LinkageError("fixture"), IllegalStateException("fixture"))) {
      var calls = 0
      try {
        planComicInfoPolicy(prepare()) { _, _, _ ->
          calls++
          throw failure
        }
        fail("Native failure hidden")
      } catch (actual: Throwable) {
        assertSame(failure, actual)
      }
      assertEquals(1, calls)
    }
  }

  @Test
  fun rejectsWrongEnvelopeAndMalformedActionsBeforeEffects() {
    val plan = ComicInfoPolicyPlan(ComicInfoAction.READ_METADATA, null, ComicInfoFetchMode.NONE)
    for ((version, module) in listOf(2 to "comicInfoPolicy", 1 to "comicInfo")) {
      assertFailsWith<IllegalArgumentException> {
        planComicInfoPolicy(prepare(), envelope(ApiEnvelope(version, module, plan)))
      }
    }
    val present = prepare().copy(metadataState = ComicInfoMetadataState.PRESENT, metadataPages = 1)
    val missing = prepare().copy(metadataState = ComicInfoMetadataState.MISSING)
    val invalid =
      listOf(
        prepare() to plan.copy(pageCount = 2),
        present to plan,
        present to plan.copy(action = ComicInfoAction.CREATE, pageCount = 2),
        present to plan.copy(action = ComicInfoAction.KEEP, pageCount = 2),
        present to plan.copy(action = ComicInfoAction.REWRITE, pageCount = 0),
        present to plan.copy(action = ComicInfoAction.REWRITE, pageCount = 3),
        missing to plan.copy(action = ComicInfoAction.CREATE, pageCount = -1),
        missing to plan.copy(action = ComicInfoAction.CREATE, pageCount = 3),
        write() to plan.copy(action = ComicInfoAction.CREATE, pageCount = 2),
        write() to plan.copy(action = ComicInfoAction.SKIP),
        write().copy(hasDirectory = false) to
          plan.copy(action = ComicInfoAction.SKIP, pageCount = 1),
        ComicInfoPolicyRequest.resolveExport(5, true) to plan,
        ComicInfoPolicyRequest.resolveExport(0, true) to
          plan.copy(action = ComicInfoAction.RESOLVE_PAGES, pageCount = 0),
        ComicInfoPolicyRequest.resolveExport(5, true) to
          plan.copy(action = ComicInfoAction.RESOLVE_PAGES, pageCount = 7),
      )
    invalid.forEach { (request, response) ->
      assertFailsWith<IllegalArgumentException> { planComicInfoPolicy(request, success(response)) }
    }
  }

  @Test
  fun rejectsNetworkPolicyNotAuthorizedByRequest() {
    val direct = ComicInfoPolicyPlan(ComicInfoAction.CREATE, null, ComicInfoFetchMode.PROPAGATE)
    val archive = direct.copy(pageCount = 2, fetchMode = ComicInfoFetchMode.CONTINUE)
    val missing = prepare().copy(metadataState = ComicInfoMetadataState.MISSING)
    for ((request, plan) in
      listOf(
        write().copy(detailedGallery = true) to direct,
        write().copy(fetchMetadata = false) to direct,
        write().copy(hasDirectory = false) to direct,
        missing to archive.copy(fetchMode = ComicInfoFetchMode.PROPAGATE),
        missing.copy(detailedGallery = true) to archive,
        missing to archive.copy(action = ComicInfoAction.KEEP),
        write() to direct.copy(fetchMode = ComicInfoFetchMode.CONTINUE),
      )) {
      assertFailsWith<IllegalArgumentException> { planComicInfoPolicy(request, success(plan)) }
    }
  }

  private fun prepare() = ComicInfoPolicyRequest.prepare(2, false, true)

  private fun write() = ComicInfoPolicyRequest.write(true, false, true)

  private fun golden(name: String) =
    File("src/main/rust/crates/domain/test_data/comic_info_policy_$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun success(plan: ComicInfoPolicyPlan) = envelope(ApiEnvelope(1, "comicInfoPolicy", plan))

  private fun envelope(envelope: ApiEnvelope<ComicInfoPolicyPlan>): ComicInfoNativeCall {
    val bytes = rustCbor.encodeToByteArray(envelope)
    return { _, _, output -> respond(output, bytes) }
  }

  private fun respond(output: ByteBuffer, bytes: ByteArray): Int {
    output.put(bytes)
    return bytes.size
  }
}
