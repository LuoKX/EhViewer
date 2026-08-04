package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ImageLimits
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.home.toLegacyLimits
import com.hippo.ehviewer.client.exception.InsufficientFundsException
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlin.random.Random
import kotlinx.serialization.Serializable

object HomeParser {
  private val TorrentKeyRegex = Regex("Your current key is: <[^>]*>([^<]*)<")
  private val FundsRegex =
    Regex("Available: ([\\d,]+) Credits.*Available: ([\\d,]+) kGP", RegexOption.DOT_MATCHES_ALL)
  private const val INSUFFICIENT_FUNDS = "Insufficient funds."

  fun parse(body: ByteBuffer) =
    parseWithAdapters(
      body = body,
      useRust = Settings.useRustCore.value,
      legacyParser = { unmarshalParsingAs<Limits>(it, ::parseLimit) },
      rustParser = {
        unmarshalParsingAs<ApiEnvelope<ImageLimits>>(it, ::parseLimitV1)
      },
    )

  fun parseResetLimits(body: String) {
    if (body.contains(INSUFFICIENT_FUNDS)) {
      throw InsufficientFundsException()
    }
  }

  fun parseTorrentKey(body: String) =
    TorrentKeyRegex.find(body)?.run { groupValues[1] }
      ?: throw ParseException("Parse torrent key error")

  fun parseFunds(body: String): Funds {
    FundsRegex.find(body)?.run {
      val fundsC = ParserUtils.parseInt(groupValues[1], 0)
      val fundsGP = ParserUtils.parseInt(groupValues[2], 0)
      return Funds(fundsGP, fundsC)
    }
    throw ParseException("Parse funds error")
  }

  internal fun parseWithAdapters(
    body: ByteBuffer,
    useRust: Boolean,
    legacyParser: (ByteBuffer) -> Limits,
    rustParser: (ByteBuffer) -> ApiEnvelope<ImageLimits>,
  ): Limits = runCatching {
    if (useRust) rustParser(body).toLegacyLimits() else legacyParser(body)
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Parse image limits error", it)
    }

  data class Result(val limits: Limits, val funds: Funds, private val id: Int = Random.nextInt())
}

@Serializable data class Funds(val gp: Int, val credit: Int)

@Serializable data class Limits(val current: Int, val maximum: Int, val resetCost: Int)

private external fun parseLimit(body: ByteBuffer, limit: Int = body.limit()): Int

/** Invokes the V1 `eh_net::home` image-limit parser. */
private external fun parseLimitV1(body: ByteBuffer, limit: Int = body.limit()): Int
