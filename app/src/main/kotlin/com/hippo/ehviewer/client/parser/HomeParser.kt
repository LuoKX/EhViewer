package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveFunds
import com.hippo.ehviewer.bridge.ImageLimits
import com.hippo.ehviewer.bridge.ImageLimitsReset
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.TorrentKey
import com.hippo.ehviewer.bridge.home.toLegacyFunds
import com.hippo.ehviewer.bridge.home.toLegacyLimits
import com.hippo.ehviewer.bridge.home.toLegacyResetLimits
import com.hippo.ehviewer.bridge.home.toLegacyTorrentKey
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlin.random.Random
import kotlinx.serialization.Serializable

object HomeParser {
  fun parse(body: ByteBuffer) =
    parseWithRustAdapter(body) {
      unmarshalParsingAs<ApiEnvelope<ImageLimits>>(it, ::parseLimitV1)
    }

  fun parseResetLimits(body: ByteBuffer) =
    unmarshalParsingAs<ApiEnvelope<ImageLimitsReset>>(body, ::parseResetLimitsV1)
      .toLegacyResetLimits()

  fun parseTorrentKey(body: ByteBuffer) =
    unmarshalParsingAs<ApiEnvelope<TorrentKey>>(body, ::parseTorrentKeyV1).toLegacyTorrentKey()

  fun parseFunds(body: ByteBuffer) =
    unmarshalParsingAs<ApiEnvelope<ArchiveFunds>>(body, ::parseFundsV1).toLegacyFunds()

  internal fun parseWithRustAdapter(
    body: ByteBuffer,
    rustParser: (ByteBuffer) -> ApiEnvelope<ImageLimits>,
  ): Limits = runCatching {
    rustParser(body).toLegacyLimits()
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Parse image limits error", it)
    }

  data class Result(val limits: Limits, val funds: Funds, private val id: Int = Random.nextInt())
}

@Serializable data class Funds(val gp: Int, val credit: Int)

@Serializable data class Limits(val current: Int, val maximum: Int, val resetCost: Int)

/** Invokes the V1 `eh_net::home` image-limit parser. */
private external fun parseLimitV1(body: ByteBuffer, limit: Int = body.limit()): Int

/** Invokes the V1 `eh_net::home` image-limit reset parser. */
private external fun parseResetLimitsV1(body: ByteBuffer, limit: Int = body.limit()): Int

/** Invokes the V1 `eh_net::home` torrent-key parser. */
private external fun parseTorrentKeyV1(body: ByteBuffer, limit: Int = body.limit()): Int

/** Invokes the V1 `eh_net::home` account-funds parser. */
private external fun parseFundsV1(body: ByteBuffer, limit: Int = body.limit()): Int
