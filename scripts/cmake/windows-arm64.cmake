# BoringSSL's MSVC assembly source list is not usable on native Windows ARM64.
set(OPENSSL_NO_ASM ON CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(CMAKE_MSVC_RUNTIME_LIBRARY MultiThreaded CACHE STRING "" FORCE)
