plugins {
  `kotlin-dsl`
  alias(libs.plugins.spotless)
}

dependencies {
  compileOnly(libs.android.gradlePlugin)
  compileOnly(libs.kotlin.gradlePlugin)
  compileOnly(libs.spotless.gradlePlugin)

  // https://github.com/gradle/gradle/issues/15383#issuecomment-779893192
  compileOnly(files(libs::class.java.superclass.protectionDomain.codeSource.location))
}

gradlePlugin {
  plugins {
    register("androidApplication") {
      id = libs.plugins.ehviewer.android.application.get().pluginId
      implementationClass = "AndroidApplicationConventionPlugin"
    }
    register("multiplatformLibrary") {
      id = libs.plugins.ehviewer.multiplatform.library.asProvider().get().pluginId
      implementationClass = "MultiplatformLibraryConventionPlugin"
    }
    register("multiplatformLibraryCompose") {
      id = libs.plugins.ehviewer.multiplatform.library.compose.get().pluginId
      implementationClass = "MultiplatformLibraryComposeConventionPlugin"
    }
  }
}

kotlin {
  val javaVersion = libs.versions.java.get().toInt()
  jvmToolchain(javaVersion)
}

spotless {
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
