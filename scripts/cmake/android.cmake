# Build dependency libraries only; benchmark probes cannot run on the host.
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(ANDROID_ABI "$ENV{GPROXY_ANDROID_ABI}" CACHE STRING "" FORCE)
set(ANDROID_PLATFORM android-28 CACHE STRING "" FORCE)
set(ANDROID_STL c++_shared CACHE STRING "" FORCE)
include("$ENV{ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake")
