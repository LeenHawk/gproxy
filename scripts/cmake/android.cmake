# Build dependency libraries only; benchmark probes cannot run on the host.
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
# Cargo passes TARGET independently to each native build, including universal APKs.
if("$ENV{TARGET}" STREQUAL "aarch64-linux-android")
  set(ANDROID_ABI arm64-v8a CACHE STRING "" FORCE)
elseif("$ENV{TARGET}" STREQUAL "x86_64-linux-android")
  set(ANDROID_ABI x86_64 CACHE STRING "" FORCE)
else()
  set(ANDROID_ABI "$ENV{GPROXY_ANDROID_ABI}" CACHE STRING "" FORCE)
endif()
set(ANDROID_PLATFORM android-28 CACHE STRING "" FORCE)
set(ANDROID_STL c++_shared CACHE STRING "" FORCE)
include("$ENV{ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake")
