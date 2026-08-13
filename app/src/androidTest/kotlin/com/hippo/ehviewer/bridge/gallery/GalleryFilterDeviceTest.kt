package com.hippo.ehviewer.bridge.gallery

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ehviewer.core.model.GalleryComment
import com.hippo.ehviewer.client.evaluateGalleryCommentContentMatches
import java.util.regex.PatternSyntaxException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI coverage for Gallery list and two-phase comment filtering. */
@RunWith(AndroidJUnit4::class)
class GalleryFilterDeviceTest {
  @Test
  fun evaluatesStoredPatternsThroughAndroidsIcuBackedPatternApi() {
    val comment = comment("かな")
    assertEquals(
      listOf(listOf<Boolean?>(true)),
      evaluateGalleryCommentContentMatches(
        filters = listOf(GalleryFilterSnapshot(5, "\\p{script=Hiragana}+", true)),
        comments = listOf(comment),
        evaluationFilterIndices = listOf(listOf(0)),
      ),
    )

    val invalidFilter = listOf(GalleryFilterSnapshot(5, "[", true))
    assertEquals(
      listOf(listOf<Boolean?>(null)),
      evaluateGalleryCommentContentMatches(
        filters = invalidFilter,
        comments = listOf(comment),
        evaluationFilterIndices = listOf(emptyList()),
      ),
    )
    assertThrows(PatternSyntaxException::class.java) {
      evaluateGalleryCommentContentMatches(
        filters = invalidFilter,
        comments = listOf(comment),
        evaluationFilterIndices = listOf(listOf(0)),
      )
    }
  }

  @Test
  fun plansListAndCommentBatchesThroughTheDefaultJniBoundary() {
    val listFilters =
      listOf(
        GalleryFilterSnapshot(0, "needle", true),
        GalleryFilterSnapshot(2, "female:tag", true),
      )
    val prePlan =
      planGalleryFilter(
        GalleryFilterRequest.galleries(
          GalleryFilterOperation.PRE_ENRICHMENT,
          listFilters,
          listOf(GalleryFilterGallery("A NEEDLE title", "user", null)),
        )
      )

    assertEquals(listOf(false), prePlan.keep)
    assertTrue(prePlan.needsTags)
    assertTrue(prePlan.commentEvaluationFilterIndices.isEmpty())

    val commentFilters =
      listOf(
        GalleryFilterSnapshot(4, "blocked", true),
        GalleryFilterSnapshot(5, "needle", true),
      )
    val comments =
      listOf(
        GalleryFilterComment(score = -100, uploader = true, commenter = "blocked"),
        GalleryFilterComment(score = 10, uploader = false, commenter = "reader"),
      )
    val preflight =
      planGalleryFilter(
        GalleryFilterRequest.comments(
          filters = commentFilters,
          comments = comments,
          scoreThreshold = 0,
        )
      )

    assertEquals(listOf(emptyList(), listOf(1)), preflight.commentEvaluationFilterIndices)
    assertTrue(preflight.keep.isEmpty())

    val finalPlan =
      planGalleryFilter(
        GalleryFilterRequest.comments(
          filters = commentFilters,
          comments = comments,
          scoreThreshold = 0,
          commentContentMatches = listOf(listOf(null, null), listOf(null, true)),
        )
      )

    assertEquals(listOf(true, false), finalPlan.keep)
    assertFalse(finalPlan.needsTags)
    assertTrue(finalPlan.commentEvaluationFilterIndices.isEmpty())
  }

  private fun comment(content: String) =
    GalleryComment(
      id = 1,
      score = 0,
      editable = false,
      voteUpAble = false,
      voteUpEd = false,
      voteDownAble = false,
      voteDownEd = false,
      uploader = false,
      voteState = null,
      time = 0,
      user = null,
      comment = content,
      lastEdited = 0,
    )
}
