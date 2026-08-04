package com.ehviewer

import com.diffplug.gradle.spotless.SpotlessExtension
import org.gradle.api.Project
import org.gradle.kotlin.dsl.apply
import org.gradle.kotlin.dsl.configure

internal fun Project.configureSpotless() {
  apply(plugin = libs.plugins.spotless.get().pluginId)

  configure<SpotlessExtension> {
    // Legacy Kotlin is reformatted as its migration slice changes. Every touched
    // file is already required to use Google style.
    ratchetFrom("origin/main")
    val ktfmtVersion = libs.versions.ktfmt.get()
    kotlin {
      // https://github.com/diffplug/spotless/issues/111
      target("src/**/*.kt")
      ktfmt(ktfmtVersion).googleStyle()
    }
    kotlinGradle {
      ktfmt(ktfmtVersion).googleStyle()
    }
  }
}
