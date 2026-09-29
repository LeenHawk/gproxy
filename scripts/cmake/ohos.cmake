set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(OHOS_ARCH "$ENV{OHOS_ARCH}" CACHE STRING "" FORCE)
set(OHOS_STL c++_shared CACHE STRING "" FORCE)
include("$ENV{OHOS_NATIVE_HOME}/build/cmake/ohos.toolchain.cmake")
