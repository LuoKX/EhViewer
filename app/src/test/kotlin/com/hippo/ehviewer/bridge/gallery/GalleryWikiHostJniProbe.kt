package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.client.EhUrl
import java.io.File

/** Explicit host probe, run with compiled test runtime classpath and lane-owned JNI library. */
object GalleryWikiHostJniProbe {
  @JvmStatic
  fun main(args: Array<String>) {
    System.load(File(args.single()).canonicalPath)
    val cases =
      listOf(
        "high" to "\uD800",
        "low" to "\uDC00",
        "pair-high" to "\uD83D\uDE00\uD800",
        "pair-low" to "\uD83D\uDE00\uDC00",
        "delimiters-space" to " a/+%#?\uD800 \uDC00 ",
      )
    var mismatches = 0
    for ((name, tag) in cases) {
      // Frozen pre-migration helper, intentionally preserving all Java UTF-16 code units.
      val oracle = "https://ehwiki.org/wiki/" + tag.replace(' ', '_')
      val actual = EhUrl.getTagDefinitionUrl(tag)
      val equal = oracle.toCharArray().contentEquals(actual.toCharArray())
      println(
        "wiki case=$name equal=$equal expectedUnits=${oracle.length} actualUnits=${actual.length}"
      )
      if (!equal) mismatches++
    }
    check(mismatches == 0) { "Wiki UTF-16 mismatch count: $mismatches" }
    println("Actual Kotlin/JNI wiki oracle: all five raw-unit cases exact")
  }
}
