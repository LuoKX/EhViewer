package com.ehviewer.core.network

import io.ktor.http.Cookie
import io.ktor.http.Url

interface CookieManager {
  fun getCookies(url: Url): Map<String, String>?

  fun setCookie(url: Url, cookie: Cookie)

  suspend fun removeAllCookies()

  fun flush()
}

expect fun getCookieManager(): CookieManager
