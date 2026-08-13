package com.hippo.ehviewer.client

import com.ehviewer.core.model.GalleryComment
import com.hippo.ehviewer.bridge.gallery.GalleryFilterSnapshot
import java.util.regex.PatternSyntaxException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class EhFilterTest {
  @Test
  fun commentFactsStayAlignedAndStopAfterTheFirstRustOrderedMatch() {
    val filters =
      listOf(
        GalleryFilterSnapshot(0, "[invalid title regex", true),
        GalleryFilterSnapshot(5, "[invalid disabled regex", false),
        GalleryFilterSnapshot(5, "foo|bar", true),
        GalleryFilterSnapshot(5, "[invalid later regex", true),
      )

    assertEquals(
      listOf(
        listOf(null, null, true, null),
        listOf(null, null, false, null),
      ),
      evaluateGalleryCommentContentMatches(
        filters = filters,
        comments = listOf(comment("a foo comment"), comment("unmatched")),
        evaluationFilterIndices = listOf(listOf(2, 3), listOf(2)),
      ),
    )
  }

  @Test
  fun invalidEnabledCommentPatternPropagatesOnlyWhenRustInstructionsReachIt() {
    assertFailsWith<PatternSyntaxException> {
      evaluateGalleryCommentContentMatches(
        filters =
          listOf(
            GalleryFilterSnapshot(5, "not-matched", true),
            GalleryFilterSnapshot(5, "[", true),
          ),
        comments = listOf(comment("content")),
        evaluationFilterIndices = listOf(listOf(0, 1)),
      )
    }
  }

  @Test
  fun invalidPatternsAreNotTouchedForRustShortCircuitedComments() {
    assertEquals(
      listOf(listOf<Boolean?>(null), listOf<Boolean?>(null), listOf<Boolean?>(null)),
      evaluateGalleryCommentContentMatches(
        filters = listOf(GalleryFilterSnapshot(5, "[", true)),
        comments = listOf(comment("uploader"), comment("threshold"), comment("commenter")),
        evaluationFilterIndices = listOf(emptyList(), emptyList(), emptyList()),
      ),
    )
  }

  @Test
  fun evaluationPlanLengthDriftFailsBeforeAnyRegexRuns() {
    assertFailsWith<IllegalArgumentException> {
      evaluateGalleryCommentContentMatches(
        filters = listOf(GalleryFilterSnapshot(5, "[", true)),
        comments = listOf(comment("content")),
        evaluationFilterIndices = emptyList(),
      )
    }
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
