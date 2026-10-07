import java.security.MessageDigest
import org.gradle.api.artifacts.component.ModuleComponentIdentifier
import org.gradle.api.tasks.Exec

plugins {
  id("com.android.application") version "9.2.1"
}

val repositoryRoot = rootProject.projectDir.resolve("../..").canonicalFile
val requestedAbis =
  providers
    .gradleProperty("nativeAbis")
    .orElse("arm64-v8a")
    .get()
    .split(",")
    .map(String::trim)
    .distinct()
val rustJobs =
  providers
    .gradleProperty("nativeJobs")
    .orElse("2")
    .get()
    .toInt()
require(
  requestedAbis.isNotEmpty() &&
    requestedAbis.all {
      it in setOf("arm64-v8a", "x86_64", "armeabi-v7a")
    },
) { "nativeAbis must contain supported Android ABIs" }
require(rustJobs > 0) { "nativeJobs must be positive" }

android {
  namespace = "moe.tarsin.ehviewer.rust.prototype"
  compileSdk = 37
  buildToolsVersion = "37.0.0"
  ndkVersion = "29.0.14206865"

  defaultConfig {
    applicationId = "moe.tarsin.ehviewer.rust.prototype"
    minSdk = 23
    targetSdk = 37
    versionCode = 2
    versionName = "0.2.0-bootstrap"
    ndk.abiFilters.addAll(requestedAbis)
  }

  sourceSets.named("main") {
    manifest.srcFile("../AndroidManifest.xml")
    jniLibs.directories.add(projectDir.resolve("../build/game-activity/jni").path)
  }

  buildFeatures {
    compose = false
    buildConfig = false
    // android-activity builds its own patched GameActivity glue.
    prefab = false
  }

  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }

  buildTypes {
    debug {
      isDebuggable = true
    }
    // Production identity/signing/cutover are not implemented by this target.
    release {
      isMinifyEnabled = false
    }
  }

  lint {
    abortOnError = true
  }
}

dependencies {
  // Exact version supported by pinned android-activity 0.6.1. Do not add prefab glue.
  implementation("androidx.games:games-activity:4.4.0")
  // GameActivity declares these as runtime dependencies; its Java superclass/interfaces
  // must also be on our loader compile classpath.
  implementation("androidx.appcompat:appcompat:1.7.1")
  implementation("androidx.core:core:1.18.0")
}

val compileRust by tasks.registering(Exec::class) {
  group = "build"
  description = "Build Rust GameActivity UI with AccessKit; stage exactly requested ABIs."
  workingDir(repositoryRoot)
  commandLine(
    listOf(
      "python3",
      "-B",
      "scripts/build_native.py",
      "--compile-only",
      "--jobs",
      rustJobs.toString(),
    ) + requestedAbis.flatMap { listOf("--abi", it) },
  )
  inputs.files(
    repositoryRoot.resolve("rust-toolchain.toml"),
    repositoryRoot.resolve("scripts/build_native.py"),
    repositoryRoot.resolve("native/Cargo.toml"),
    repositoryRoot.resolve("native/Cargo.lock"),
    repositoryRoot.resolve("app/src/main/rust/Cargo.toml"),
  )
  inputs.dir(repositoryRoot.resolve("native/src"))
  inputs.dir(repositoryRoot.resolve("app/src/main/rust/crates/net"))
  inputs.dir(repositoryRoot.resolve("app/src/main/rust/crates/domain"))
  inputs.property("abis", requestedAbis)
  inputs.property("jobs", rustJobs)
  outputs.dir(repositoryRoot.resolve("native/build/game-activity/jni"))
  // Cargo tracks target flags, environment and dependencies more completely than Gradle.
  // Always validate exports and restage; Cargo itself skips unchanged compilations.
  outputs.upToDateWhen { false }
}

val frameworkOrigins = layout.buildDirectory.file("framework-artifacts.tsv")
val recordFrameworkOrigins by tasks.registering {
  val runtimeClasspath = configurations.named("debugRuntimeClasspath")
  inputs.files(runtimeClasspath)
  outputs.file(frameworkOrigins)
  doLast {
    val rows =
      runtimeClasspath
        .get()
        .incoming.artifacts.artifacts
        .map { artifact ->
          val id =
            artifact.id.componentIdentifier as? ModuleComponentIdentifier
              ?: error("Bootstrap JVM runtime must contain only external framework modules")
          require(
            id.group.startsWith("androidx.") ||
              id.group in
              setOf(
                "org.jetbrains",
                "org.jetbrains.kotlin",
                "org.jetbrains.kotlinx",
                "org.jspecify",
                "com.google.guava",
              ),
          ) { "Unreviewed framework dependency: $id" }
          val file = artifact.file.canonicalFile
          require(!file.path.contains('\t') && !file.path.contains('\n'))
          val hash =
            MessageDigest
              .getInstance("SHA-256")
              .digest(file.readBytes())
              .joinToString("") { "%02x".format(it) }
          "${id.group}:${id.module}:${id.version}\t${file.path}\t$hash"
        }.sorted()
    frameworkOrigins.get().asFile.writeText(rows.joinToString("\n", postfix = "\n"))
  }
}

tasks.named("preBuild") {
  dependsOn(compileRust, recordFrameworkOrigins)
}
