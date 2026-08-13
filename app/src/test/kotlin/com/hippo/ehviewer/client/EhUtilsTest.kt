package com.hippo.ehviewer.client

import kotlin.test.Test
import kotlin.test.assertEquals

class EhUtilsTest {
  @Test
  fun categoryBitsAndPresentationLabelsMatchTheRustGolden() {
    val golden =
      listOf(
        EhUtils.MISC to "misc",
        EhUtils.DOUJINSHI to "doujinshi",
        EhUtils.MANGA to "manga",
        EhUtils.ARTIST_CG to "artistcg",
        EhUtils.GAME_CG to "gamecg",
        EhUtils.IMAGE_SET to "imageset",
        EhUtils.COSPLAY to "cosplay",
        EhUtils.ASIAN_PORN to "asianporn",
        EhUtils.NON_H to "non-h",
        EhUtils.WESTERN to "western",
        EhUtils.PRIVATE to "private",
        EhUtils.UNKNOWN to "unknown",
      )

    assertEquals(-1, EhUtils.NONE)
    assertEquals(0x3ff, EhUtils.ALL_CATEGORY)
    assertEquals(
      listOf(1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048),
      golden.map(Pair<Int, String>::first),
    )
    golden.forEach { (category, label) -> assertEquals(label, EhUtils.getCategory(category)) }
    assertEquals("unknown", EhUtils.getCategory(EhUtils.NONE))
    assertEquals("unknown", EhUtils.getCategory(Int.MAX_VALUE))
  }
}
