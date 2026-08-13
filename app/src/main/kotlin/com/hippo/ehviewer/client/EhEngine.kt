/*
 * Copyright 2016 Hippo Seven
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.client

import arrow.core.Either
import arrow.core.left
import arrow.core.partially2
import arrow.core.right
import arrow.fx.coroutines.parMap
import arrow.fx.coroutines.parZip
import com.ehviewer.core.i18n.R
import com.ehviewer.core.model.BaseGalleryInfo
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.model.GalleryTagGroup
import com.ehviewer.core.network.EhCookieStore
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.auth.SessionFailureDecisionKind
import com.hippo.ehviewer.bridge.auth.SessionFailurePlan
import com.hippo.ehviewer.bridge.auth.SessionFailureSnapshot
import com.hippo.ehviewer.bridge.auth.SessionFailureSource
import com.hippo.ehviewer.bridge.auth.SessionGallerySite
import com.hippo.ehviewer.bridge.auth.classifySessionFailure
import com.hippo.ehviewer.bridge.gallery.FavoriteMutationBulk
import com.hippo.ehviewer.bridge.gallery.FavoriteMutationRequest
import com.hippo.ehviewer.bridge.gallery.FavoriteMutationSingle
import com.hippo.ehviewer.bridge.gallery.GALLERY_ARCHIVE_UNAVAILABLE_MESSAGE
import com.hippo.ehviewer.bridge.gallery.GalleryArchiveNullOutcome
import com.hippo.ehviewer.bridge.gallery.GalleryArchiveParsedOutcome
import com.hippo.ehviewer.bridge.gallery.GalleryArchiveRequest
import com.hippo.ehviewer.bridge.gallery.GalleryArchiveRequestKind
import com.hippo.ehviewer.bridge.gallery.GalleryUrlSite
import com.hippo.ehviewer.bridge.gallery.planFavoriteMutation
import com.hippo.ehviewer.bridge.gallery.planGalleryArchiveDownloadPolicy
import com.hippo.ehviewer.bridge.gallery.planGalleryArchiveRequest
import com.hippo.ehviewer.bridge.gallery.planGalleryMetadataPolicy
import com.hippo.ehviewer.bridge.gallery.toRequestWaves
import com.hippo.ehviewer.bridge.toLegacyError
import com.hippo.ehviewer.bridge.toRustError
import com.hippo.ehviewer.client.data.fillInfo
import com.hippo.ehviewer.client.data.filterComments
import com.hippo.ehviewer.client.exception.CloudflareBypassException
import com.hippo.ehviewer.client.exception.EhError
import com.hippo.ehviewer.client.exception.EhException
import com.hippo.ehviewer.client.exception.InsufficientFundsException
import com.hippo.ehviewer.client.exception.InsufficientGpException
import com.hippo.ehviewer.client.exception.IpBannedException
import com.hippo.ehviewer.client.exception.NoHathClientException
import com.hippo.ehviewer.client.exception.NoHitsFoundException
import com.hippo.ehviewer.client.exception.NotLoggedInException
import com.hippo.ehviewer.client.exception.ParseException
import com.hippo.ehviewer.client.parser.ArchiveParser
import com.hippo.ehviewer.client.parser.EventPaneParser
import com.hippo.ehviewer.client.parser.FavParserResult
import com.hippo.ehviewer.client.parser.FavoritesParser
import com.hippo.ehviewer.client.parser.GalleryApiParser
import com.hippo.ehviewer.client.parser.GalleryDetailParser
import com.hippo.ehviewer.client.parser.GalleryListParser
import com.hippo.ehviewer.client.parser.GalleryMultiPageViewerPTokenParser
import com.hippo.ehviewer.client.parser.GalleryPageParser
import com.hippo.ehviewer.client.parser.GalleryTokenApiParser
import com.hippo.ehviewer.client.parser.HomeParser
import com.hippo.ehviewer.client.parser.ProfileParser
import com.hippo.ehviewer.client.parser.RateGalleryParser
import com.hippo.ehviewer.client.parser.RateGalleryResult
import com.hippo.ehviewer.client.parser.SignInParser
import com.hippo.ehviewer.client.parser.TorrentParser
import com.hippo.ehviewer.client.parser.TorrentResult
import com.hippo.ehviewer.client.parser.UserConfigParser
import com.hippo.ehviewer.client.parser.VoteCommentParser
import com.hippo.ehviewer.client.parser.VoteCommentResult
import com.hippo.ehviewer.client.parser.VoteTagParser
import com.hippo.ehviewer.dailycheck.showEventNotification
import com.hippo.ehviewer.dailycheck.today
import com.hippo.ehviewer.util.AppConfig
import com.hippo.ehviewer.util.ReadableTime
import com.hippo.ehviewer.util.bodyAsUtf8Text
import com.hippo.ehviewer.util.ensureSuccess
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.HttpStatement
import io.ktor.client.statement.bodyAsChannel
import io.ktor.client.statement.request
import io.ktor.http.Url
import io.ktor.util.moveToByteArray
import io.ktor.utils.io.pool.DirectByteBufferPool
import io.ktor.utils.io.pool.useInstance
import io.ktor.utils.io.readAvailable
import java.io.File
import java.nio.ByteBuffer
import kotlin.math.ceil
import kotlin.system.measureTimeMillis
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.serialization.SerializationException
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.json.add
import kotlinx.serialization.json.addJsonArray
import kotlinx.serialization.json.put
import moe.tarsin.coroutines.runSuspendCatching
import okio.buffer
import okio.sink
import splitties.init.appCtx

private val Url.isLogin
  get() = segments.firstOrNull() == "bounce_login.php"

private fun Either<String, ByteBuffer>.saveParseError(e: Throwable) {
  val dir = AppConfig.externalParseErrorDir ?: return
  val file = File(dir, ReadableTime.getFilenamableTime() + ".txt")
  file.sink().buffer().use { sink ->
    with(sink) {
      writeUtf8(e.message + "\n")
      onLeft { writeUtf8(it) }
      onRight { write(it) }
    }
  }
}

private fun rethrowExactly(
  response: HttpResponse,
  body: Either<String, ByteBuffer>,
  e: Throwable,
): Nothing {
  // Cancellation is a coroutine control signal and never enters a native policy.
  if (e is CancellationException) throw e

  val plan = classifySessionFailure(buildSessionFailureSnapshot(response, body, e))
  applySessionFailurePlan(plan, response, body, e)
}

private fun buildSessionFailureSnapshot(
  response: HttpResponse,
  body: Either<String, ByteBuffer>,
  error: Throwable,
): SessionFailureSnapshot {
  val cloudflareChallenge = response.headers["cf-mitigated"] == "challenge"
  val emptyBody = body.fold(String::isEmpty) { !it.hasRemaining() }
  val parserFailure = error is ParseException || error is SerializationException
  val domainError =
    if (cloudflareChallenge || emptyBody) {
      null
    } else {
      (error as? RustFailureException)?.failure
        ?: body
          .getOrNull()
          ?.runCatching {
            val array = moveToByteArray()
            rewind()
            Cbor.decodeFromByteArray<EhError>(array)
          }
          ?.getOrNull()
          ?.toRustError()
    }
  return SessionFailureSnapshot(
    site =
      if (response.request.url.host == EhUrl.DOMAIN_EX) SessionGallerySite.EX_HENTAI
      else SessionGallerySite.PUBLIC,
    cloudflareChallenge = cloudflareChallenge,
    emptyBody = emptyBody,
    domainError = domainError,
    httpStatusSuccess = response.status.value in 200..299,
    source =
      if (parserFailure) {
        SessionFailureSource.parse(nonHtmlText = body.leftOrNull()?.contains('<') == false)
      } else {
        SessionFailureSource.other()
      },
  )
}

private fun applySessionFailurePlan(
  plan: SessionFailurePlan,
  response: HttpResponse,
  body: Either<String, ByteBuffer>,
  cause: Throwable,
): Nothing =
  when (plan.decision.kind) {
    SessionFailureDecisionKind.CLOUDFLARE_CHALLENGE -> throw CloudflareBypassException()
    SessionFailureDecisionKind.EMPTY_BODY_IP_BANNED -> {
      val message =
        if (plan.decision.site == SessionGallerySite.EX_HENTAI) {
          "Sad Panda\n(without panda)"
        } else {
          appCtx.getString(R.string.error_ip_banned)
        }
      throw IpBannedException(message)
    }
    SessionFailureDecisionKind.DOMAIN ->
      throwLegacyDomainError(checkNotNull(plan.decision.error).toLegacyError())
    SessionFailureDecisionKind.HTTP_STATUS -> {
      response.status.ensureSuccess()
      error("Session Policy selected HTTP failure for a successful status")
    }
    SessionFailureDecisionKind.NON_HTML_TEXT_IP_BANNED ->
      throw IpBannedException(checkNotNull(body.leftOrNull()))
    SessionFailureDecisionKind.PARSE_FAILURE -> {
      if (Settings.saveParseErrorBody.value) body.saveParseError(cause)
      throw EhException(appCtx.getString(R.string.error_parse_error), cause)
    }
    SessionFailureDecisionKind.RETHROW -> throw cause
  }

private fun throwLegacyDomainError(error: EhError): Nothing =
  when (error) {
    EhError.NoHits -> throw NoHitsFoundException()
    EhError.NoWatched -> throw EhException(R.string.gallery_list_empty_hit_subscription)
    EhError.NeedLogin -> throw NotLoggedInException()
    EhError.NoHathClient -> throw NoHathClientException()
    EhError.InsufficientFunds -> throw InsufficientFundsException()
    is EhError.GalleryUnavailable -> throw EhException(error.message)
    is EhError.IpBanned -> throw IpBannedException(error.message)
    is EhError.Error -> throw EhException(error.message)
  }

private val httpContentPool = DirectByteBufferPool(8, 0x80000)

private suspend inline fun <T> HttpStatement.fetchUsingAsByteBuffer(
  crossinline block: suspend ByteBuffer.() -> T
) = executeSafely { response ->
  if (response.request.url.isLogin) throw NotLoggedInException()
  httpContentPool.useInstance { buffer ->
    with(response.bodyAsChannel()) { while (!isClosedForRead) readAvailable(buffer) }
    buffer.flip()
    runSuspendCatching {
      block(buffer)
    }
      .onFailure {
        buffer.rewind()
        rethrowExactly(response, buffer.right(), it)
      }
      .getOrThrow()
  }
}

object EhEngine {
  suspend fun getOriginalImageUrl(url: String, referer: String?) =
    noRedirectEhRequest(url, referer).executeSafely { response ->
      val location =
        response.headers["Location"]
          ?: run {
            val body = response.bodyAsUtf8Text()
            rethrowExactly(response, body.left(), InsufficientGpException())
          }
      if (Url(location).isLogin) throw InsufficientGpException()
      location
    }

  suspend fun getPTokenListFromMpv(gid: Long, token: String): List<String> {
    val request = ehRequest(EhUrl.getGalleryMultiPageViewerUrl(gid, token), EhUrl.referer)
    return request.fetchUsingAsByteBuffer(GalleryMultiPageViewerPTokenParser::parseV1)
  }

  suspend fun getTorrentList(gid: Long, token: String): TorrentResult {
    val url = EhUrl.getTorrentUrl(gid, token)
    val referer = EhUrl.getGalleryDetailUrl(gid, token)
    return ehRequest(url, referer).fetchUsingAsByteBuffer(TorrentParser::parse)
  }

  suspend fun getTorrentKey() =
    EhCookieStore.getUserId()?.let { id ->
      val key = ehRequest(EhUrl.URL_HOME).fetchUsingAsByteBuffer(HomeParser::parseTorrentKey)
      "${id}x$key"
    }

  suspend fun getArchiveList(gid: Long, token: String): ArchiveParser.Result {
    val site = if (EhUtils.isExHentai) GalleryUrlSite.EX_HENTAI else GalleryUrlSite.PUBLIC
    val plan =
      planGalleryArchiveRequest(
        GalleryArchiveRequest(1, site, GalleryArchiveRequestKind.LIST, gid, token)
      )
    val url = plan.url
    val funds = if (EhUtils.isExHentai) getFunds() else null
    return ehRequest(url, plan.referer)
      .fetchUsingAsByteBuffer(ArchiveParser::parse.partially2(funds))
  }

  suspend fun getImageLimits() =
    parZip(
      { ehRequest(EhUrl.URL_HOME).fetchUsingAsByteBuffer(HomeParser::parse) },
      { getFunds() },
      { limits, funds -> HomeParser.Result(limits, funds) },
    )

  private suspend fun getFunds() =
    ehRequest(EhUrl.URL_FUNDS).fetchUsingAsByteBuffer(HomeParser::parseFunds)

  suspend fun getNews(parse: Boolean) =
    ehRequest(EhUrl.URL_NEWS, EhUrl.REFERER_E).fetchUsingAsByteBuffer {
      if (parse) EventPaneParser.parse(this) else null
    }

  suspend fun getProfile(): ProfileParser.Result {
    val url = ehRequest(EhUrl.URL_FORUMS).fetchUsingAsByteBuffer(ProfileParser::parseProfileUrl)
    return ehRequest(url, EhUrl.URL_FORUMS).fetchUsingAsByteBuffer(ProfileParser::parse)
  }

  suspend fun getUConfig(gallerySite: Int) {
    val url = EhUrl.getUConfigUrl(gallerySite)
    runSuspendCatching {
      ehRequest(url).fetchUsingAsByteBuffer(UserConfigParser::parse)
    }
      .onFailure { throwable ->
        // It may get redirected when accessing ex for the first time
        if (url == EhUrl.URL_UCONFIG_EX) {
          logcat(throwable)
          ehRequest(url).fetchUsingAsByteBuffer(UserConfigParser::parse)
        } else {
          throw throwable
        }
      }
  }

  suspend fun getGalleryPage(url: String, gid: Long, token: String): GalleryPageParser.Result {
    val referer = EhUrl.getGalleryDetailUrl(gid, token)
    val request = ehRequest(url, referer)
    return request.fetchUsingAsByteBuffer(GalleryPageParser::parseV1)
  }

  suspend fun getGalleryList(url: String) =
    ehRequest(url, EhUrl.referer)
      .fetchUsingAsByteBuffer(GalleryListParser::parse)
      .apply { galleryInfoList.fillInfo(url, true) }
      .takeUnless { it.galleryInfoList.isEmpty() } ?: GalleryListParser.emptyResult

  /** Recovery-only neutral list request: no browsing filters, favorites, or implicit gdata. */
  internal suspend fun getRecoveryGalleryList(url: String) =
    ehRequest(url, EhUrl.referer).fetchUsingAsByteBuffer(GalleryListParser::parse).takeUnless {
      it.galleryInfoList.isEmpty()
    } ?: GalleryListParser.emptyResult

  suspend fun getGalleryDetail(url: String) =
    ehRequest(url, EhUrl.referer).fetchUsingAsByteBuffer(GalleryDetailParser::parse).run {
      event?.let {
        Settings.lastDawnDays = today
        showEventNotification(it)
      }
      detail.apply {
        // Fill info for database
        fillInfo()
        filterComments()
      }
    }

  suspend fun getPreviewList(url: String) =
    ehRequest(url, EhUrl.referer).fetchUsingAsByteBuffer(GalleryDetailParser::parsePreviews)

  suspend fun getFavorites(url: String) =
    ehRequest(url, EhUrl.referer).fetchUsingAsByteBuffer(FavoritesParser::parse).apply {
      galleryInfoList.fillInfo(url)
    }

  suspend fun signIn(username: String, password: String): String {
    val referer = "https://forums.e-hentai.org/index.php?act=Login&CODE=00"
    val url = EhUrl.API_SIGN_IN
    val origin = "https://forums.e-hentai.org"
    val request =
      ehRequest(url, referer, origin) {
        formBody {
          append("referer", referer)
          append("b", "")
          append("bt", "")
          append("UserName", username)
          append("PassWord", password)
          append("CookieDate", "1")
        }
      }
    return request.fetchUsingAsByteBuffer(SignInParser::parseV1)
  }

  suspend fun commentGallery(url: String, comment: String, id: Long = -1) =
    ehRequest(url, url, EhUrl.origin) {
        formBody {
          if (id == -1L) {
            append("commenttext_new", comment)
          } else {
            append("commenttext_edit", comment)
            append("edit_comment", id.toString())
          }
        }
      }
      .executeSafely { response ->
        // Ktor does not handle POST redirect, we need to do it manually
        // https://youtrack.jetbrains.com/issue/KTOR-478
        val location = response.headers["Location"] ?: url
        ehRequest(location, url).fetchUsingAsByteBuffer(GalleryDetailParser::parseComments)
      }

  suspend fun modifyFavorites(gid: Long, token: String, dstCat: Int = -1, note: String = "") {
    if (dstCat != -1 && dstCat !in 0..9) throw EhException("Invalid dstCat: $dstCat")
    val selectedPlan =
      planFavoriteMutation(
        FavoriteMutationRequest(
          1,
          if (EhUtils.isExHentai) GalleryUrlSite.EX_HENTAI else GalleryUrlSite.PUBLIC,
          single = FavoriteMutationSingle(gid, token, dstCat, note),
          bulk = null,
        )
      )
    ehRequest(selectedPlan.url, selectedPlan.referer, selectedPlan.origin) {
        formBody {
          selectedPlan.form.forEach { append(it.name, it.value) }
        }
      }
      .executeSafely {}
  }

  suspend fun getFavoriteNote(gid: Long, token: String) =
    ehRequest(EhUrl.getAddFavorites(gid, token), EhUrl.getGalleryDetailUrl(gid, token))
      .fetchUsingAsByteBuffer(FavoritesParser::parseNote)

  suspend fun downloadArchive(gid: Long, token: String, res: String, isHath: Boolean): String? {
    val site = if (EhUtils.isExHentai) GalleryUrlSite.EX_HENTAI else GalleryUrlSite.PUBLIC
    val plan =
      planGalleryArchiveRequest(
        GalleryArchiveRequest(
          1,
          site,
          GalleryArchiveRequestKind.DOWNLOAD,
          gid,
          token,
          res,
          isHath,
        )
      )
    val networkPolicy = planGalleryArchiveDownloadPolicy(isHath)
    val request =
      ehRequest(plan.url, plan.referer, plan.origin) {
        formBody {
          plan.form.forEach { append(it.name, it.value) }
        }
      }
    for (attempt in networkPolicy.attempts) {
      if (attempt.delayBeforeMillis != 0L) delay(attempt.delayBeforeMillis)
      val result = request.fetchUsingAsByteBuffer(ArchiveParser::parseArchiveUrl)
      if (result != null) {
        return when (networkPolicy.parsedOutcome) {
          GalleryArchiveParsedOutcome.RETURN_URL -> result
          GalleryArchiveParsedOutcome.RETURN_NULL -> null
        }
      }
    }
    return when (networkPolicy.exhaustedNullOutcome) {
      GalleryArchiveNullOutcome.RETURN_NULL -> null
      GalleryArchiveNullOutcome.ARCHIVE_UNAVAILABLE ->
        throw EhException(GALLERY_ARCHIVE_UNAVAILABLE_MESSAGE)
    }
  }

  suspend fun resetImageLimits() =
    ehRequest(EhUrl.URL_HOME) {
        formBody {
          append("reset_imagelimit", "Reset Quota")
        }
      }
      .fetchUsingAsByteBuffer(HomeParser::parseResetLimits)

  suspend fun modifyFavorites(gidArray: LongArray, srcCat: Int, dstCat: Int): FavParserResult {
    if (dstCat != -1 && dstCat !in 0..9) throw EhException("Invalid dstCat: $dstCat")
    val selectedPlan =
      planFavoriteMutation(
        FavoriteMutationRequest(
          1,
          if (EhUtils.isExHentai) GalleryUrlSite.EX_HENTAI else GalleryUrlSite.PUBLIC,
          single = null,
          bulk = FavoriteMutationBulk(gidArray.toList(), srcCat, dstCat),
        )
      )
    return ehRequest(selectedPlan.url, selectedPlan.referer, selectedPlan.origin) {
        formBody {
          selectedPlan.form.forEach { append(it.name, it.value) }
        }
      }
      .fetchUsingAsByteBuffer(FavoritesParser::parse)
      .apply { galleryInfoList.fillInfo(selectedPlan.url) }
  }

  suspend fun getGalleryPageApi(
    gid: Long,
    index: Int,
    pToken: String,
    showKey: String?,
    previousPToken: String?,
  ): GalleryPageParser.Result {
    val referer =
      if (index > 0 && previousPToken != null) EhUrl.getPageUrl(gid, index - 1, previousPToken)
      else null
    val request =
      ehRequest(EhUrl.apiUrl, referer, EhUrl.origin) {
        jsonBody {
          put("method", "showpage")
          put("gid", gid)
          put("page", index + 1)
          put("imgkey", pToken)
          put("showkey", showKey)
        }
      }
    return request.fetchUsingAsByteBuffer(GalleryPageParser::parseV1)
  }

  suspend fun rateGallery(
    apiUid: Long,
    apiKey: String?,
    gid: Long,
    token: String,
    rating: Float,
  ): RateGalleryResult {
    val request =
      ehRequest(EhUrl.apiUrl, EhUrl.getGalleryDetailUrl(gid, token), EhUrl.origin) {
        jsonBody {
          put("method", "rategallery")
          put("apiuid", apiUid)
          put("apikey", requireNotNull(apiKey))
          put("gid", gid)
          put("token", token)
          put("rating", ceil((rating * 2).toDouble()).toInt())
        }
      }
    return request.fetchUsingAsByteBuffer(RateGalleryParser::parseV1)
  }

  suspend fun fillGalleryListByApi(
    galleryInfoList: List<GalleryInfo>,
    referer: String? = null,
  ) {
    val policy = planGalleryMetadataPolicy(galleryInfoList.size)
    policy.toRequestWaves(galleryInfoList).forEachIndexed { index, chunk ->
      if (index != 0) {
        delay(policy.waveDelayMillis)
      }
      chunk.parMap { galleries ->
        val request =
          ehRequest(EhUrl.apiUrl, referer, EhUrl.origin) {
            jsonBody {
              put("method", "gdata")
              array("gidlist") {
                galleries.forEach { info ->
                  addJsonArray {
                    add(info.gid)
                    add(info.token)
                  }
                }
              }
              put("namespace", 1)
            }
          }
        val batch = request.fetchUsingAsByteBuffer(GalleryApiParser::parseV1)
        GalleryApiParser.applyV1(batch, galleries)
      }
    }
  }

  suspend fun voteComment(
    apiUid: Long,
    apiKey: String?,
    gid: Long,
    token: String,
    commentId: Long,
    commentVote: Int,
  ): VoteCommentResult {
    val request =
      ehRequest(EhUrl.apiUrl, EhUrl.referer, EhUrl.origin) {
        jsonBody {
          put("method", "votecomment")
          put("apiuid", apiUid)
          put("apikey", requireNotNull(apiKey))
          put("gid", gid)
          put("token", token)
          put("comment_id", commentId)
          put("comment_vote", commentVote)
        }
      }
    return request.fetchUsingAsByteBuffer(VoteCommentParser::parseV1)
  }

  suspend fun voteTag(
    apiUid: Long,
    apiKey: String?,
    gid: Long,
    token: String,
    tags: String,
    vote: Int,
  ): List<GalleryTagGroup> {
    val request =
      ehRequest(EhUrl.apiUrl, EhUrl.referer, EhUrl.origin) {
        jsonBody {
          put("method", "taggallery")
          put("apiuid", apiUid)
          put("apikey", requireNotNull(apiKey))
          put("gid", gid)
          put("token", token)
          put("tags", tags)
          put("vote", vote)
        }
      }
    return request.fetchUsingAsByteBuffer(VoteTagParser::parseV1)
  }

  suspend fun getGalleryToken(
    gid: Long,
    gtoken: String,
    page: Int,
  ): String {
    val request =
      ehRequest(EhUrl.apiUrl, EhUrl.referer, EhUrl.origin) {
        jsonBody {
          put("method", "gtoken")
          array("pagelist") {
            addJsonArray {
              add(gid)
              add(gtoken)
              add(page + 1)
            }
          }
        }
      }
    return request.fetchUsingAsByteBuffer(GalleryTokenApiParser::parseV1)
  }

  private suspend fun MutableList<BaseGalleryInfo>.fillInfo(url: String, filter: Boolean = false) {
    val needsTags = filter && EhFilter.filterPreEnrichment(this)
    val hasTags = any { !it.simpleTags.isNullOrEmpty() }
    val hasPages = any { it.pages != 0 }
    val hasRated = any { it.rated }
    val needApi =
      filter && needsTags && !hasTags || Settings.showGalleryPages.value && !hasPages || hasRated
    if (needApi) fillGalleryListByApi(this, url)
    if (filter) EhFilter.filterPostEnrichment(this)
    forEach {
      if (it.favoriteSlot == GalleryInfo.NOT_FAVORITED && EhDB.containLocalFavorites(it.gid)) {
        it.favoriteSlot = GalleryInfo.LOCAL_FAVORITED
      }
      if (!needApi) it.generateSLang()
    }
  }

  suspend fun addFavorites(galleryList: List<Pair<Long, String>>, dstCat: Int) {
    galleryList.forEach { (gid, token) ->
      // https://github.com/FooIbar/EhViewer/issues/1190
      // Workaround for duplicate items when sorting by favorited time
      val timeTaken = measureTimeMillis {
        modifyFavorites(gid, token, dstCat)
      }
      delay(1000 - timeTaken)
    }
  }
}
