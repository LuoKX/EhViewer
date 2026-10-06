cmake_minimum_required(VERSION 3.14)
project(ehviewer C)
include(FetchContent)

if (NOT CMAKE_BUILD_TYPE STREQUAL "Debug")
    string(CONCAT CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O3 -ffast-math -fvisibility=hidden -fvisibility-inlines-hidden -funroll-loops -flto "
            "-ffile-prefix-map=${ANDROID_TOOLCHAIN_ROOT}= "
            "-ffile-prefix-map=${CMAKE_SOURCE_DIR}= "
            "-ffile-prefix-map=${CMAKE_BINARY_DIR}= "
            "-mllvm -polly "
            "-mllvm -polly-run-dce "
            "-mllvm -polly-run-inliner "
            "-mllvm -polly-isl-arg=--no-schedule-serialize-sccs "
            "-mllvm -polly-ast-use-context "
            "-mllvm -polly-detect-keep-going "
            "-mllvm -polly-position=before-vectorizer "
            "-mllvm -polly-vectorizer=stripmine "
            "-mllvm -polly-detect-profitability-min-per-loop-insts=40 "
            "-mllvm -polly-invariant-load-hoisting")
endif (NOT CMAKE_BUILD_TYPE STREQUAL "Debug")

set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ../../../../rust-toolchain.toml)
unset(Rust_TOOLCHAIN CACHE)
FetchContent_Declare(
        Corrosion
        GIT_REPOSITORY https://github.com/FooIbar/corrosion.git
        GIT_TAG v0.6.1
        GIT_SHALLOW 1
)

FetchContent_MakeAvailable(Corrosion)
corrosion_import_crate(
        MANIFEST_PATH ../rust/Cargo.toml
        LOCKED
        FEATURES $<IF:$<VERSION_GREATER_EQUAL:${ANDROID_NATIVE_API_LEVEL},26>,android-26,android>
        FLAGS $<$<NOT:$<CONFIG:Debug>>:-Z build-std -Z build-std-features=>
)
corrosion_add_target_rustflags(
        ehviewer_rust
        -Clink-arg=--target=${CMAKE_C_COMPILER_TARGET}
        $<$<NOT:$<CONFIG:Debug>>:-Z unstable-options -C panic=immediate-abort -Z location-detail=none -Z fmt-debug=none>
)
corrosion_link_libraries(ehviewer_rust android jnigraphics log)

# Build the behavior-free app wrapper and link the Rust-owned native surfaces.
add_library(${PROJECT_NAME} SHARED link_anchor.c)
target_link_libraries(
        ${PROJECT_NAME} ehviewer_rust
        -Wl,-u,JNI_OnLoad -Wl,--undefined-glob,Java_*
        -Wl,--version-script,${CMAKE_SOURCE_DIR}/libehviewer.map.txt
)
