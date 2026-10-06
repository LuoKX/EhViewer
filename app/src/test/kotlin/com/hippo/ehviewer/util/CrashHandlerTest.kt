package com.hippo.ehviewer.util

import java.io.PrintWriter
import java.io.StringWriter
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertEquals

class CrashHandlerTest {
  @Test
  fun printsEachCauseOnlyOnce() {
    val cause = IllegalStateException("root").apply { stackTrace = emptyArray() }
    val failure = IllegalArgumentException("outer", cause).apply { stackTrace = emptyArray() }

    val trace = render(failure)
    assertEquals(
      1,
      trace.lineSequence().count { it.endsWith("java.lang.IllegalStateException: root") },
    )
    assertContains(trace, "Caused by: java.lang.IllegalStateException: root")
  }

  @Test
  fun terminatesForCyclicCauseChains() {
    val first = IllegalStateException("first").apply { stackTrace = emptyArray() }
    val second = IllegalArgumentException("second").apply { stackTrace = emptyArray() }
    first.initCause(second)
    second.initCause(first)

    assertContains(render(first), "CIRCULAR REFERENCE")
  }

  @Test
  fun preservesSuppressedExceptions() {
    val failure = IllegalStateException("outer").apply { stackTrace = emptyArray() }
    failure.addSuppressed(IOExceptionForTest())

    assertContains(render(failure), "Suppressed:")
    assertContains(render(failure), "suppressed failure")
  }

  private fun render(failure: Throwable): String {
    val output =
      object : StringWriter() {
        override fun write(str: String, off: Int, len: Int) {
          // Bound regressions so a cyclic cause cannot hang the test or exhaust heap space.
          if (buffer.length + len > 16_384)
            throw AssertionError("Exception chain printed repeatedly")
          super.write(str, off, len)
        }
      }
    val method =
      CrashHandler::class
        .java
        .getDeclaredMethod(
          "getThrowableInfo",
          Throwable::class.java,
          PrintWriter::class.java,
        )
        .apply { isAccessible = true }
    method.invoke(CrashHandler, failure, PrintWriter(output))
    return output.toString()
  }

  private class IOExceptionForTest : RuntimeException("suppressed failure") {
    init {
      stackTrace = emptyArray()
    }
  }
}
