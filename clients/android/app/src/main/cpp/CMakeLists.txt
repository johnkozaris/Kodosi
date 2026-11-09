cmake_minimum_required(VERSION 4.1)
project(kodosi_android C)

set(KODOSI_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../..")
set(KODOSI_GHOSTTY_VT "${KODOSI_ROOT}/terminal/ghostty/Vendor/GhosttyVt/android-arm64")
set(KODOSI_GHOSTTY_ARCHIVE "${KODOSI_GHOSTTY_VT}/lib/libghostty-vt.a")
set(KODOSI_RUNTIME_LIBRARY "${KODOSI_RUNTIME_DIR}/${ANDROID_ABI}/libkodosi_ffi_c.so")

if(NOT EXISTS "${KODOSI_RUNTIME_LIBRARY}")
    message(FATAL_ERROR "The Android runtime is missing: ${KODOSI_RUNTIME_LIBRARY}. Run just android-runtime-build.")
endif()

file(STRINGS "${KODOSI_ROOT}/Ghostty.lock" KODOSI_GHOSTTY_PIN REGEX "^android_arm64_archive_sha256=")
string(REPLACE "android_arm64_archive_sha256=" "" KODOSI_GHOSTTY_PIN "${KODOSI_GHOSTTY_PIN}")
file(SHA256 "${KODOSI_GHOSTTY_ARCHIVE}" KODOSI_GHOSTTY_DIGEST)
if(NOT KODOSI_GHOSTTY_DIGEST STREQUAL KODOSI_GHOSTTY_PIN)
    message(FATAL_ERROR "${KODOSI_GHOSTTY_ARCHIVE} does not match android_arm64_archive_sha256 in Ghostty.lock.")
endif()
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${KODOSI_ROOT}/Ghostty.lock")

add_library(kodosi_runtime SHARED IMPORTED)
set_target_properties(kodosi_runtime PROPERTIES IMPORTED_LOCATION "${KODOSI_RUNTIME_LIBRARY}")

add_library(ghostty_vt STATIC IMPORTED)
set_target_properties(ghostty_vt PROPERTIES
    IMPORTED_LOCATION "${KODOSI_GHOSTTY_ARCHIVE}"
    INTERFACE_INCLUDE_DIRECTORIES "${KODOSI_GHOSTTY_VT}/include"
    INTERFACE_COMPILE_DEFINITIONS GHOSTTY_STATIC
)

add_library(kodosi_android SHARED runtime_bridge.c terminal_kernel.c)
set_target_properties(kodosi_android PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED ON)
target_include_directories(kodosi_android PRIVATE "${KODOSI_ROOT}/clients/apple/Sources/KodosiKit")
target_compile_options(kodosi_android PRIVATE -Wall -Wextra -Werror -fvisibility=hidden)
target_link_options(kodosi_android PRIVATE -Wl,--exclude-libs,ALL -Wl,--no-undefined)
target_link_libraries(kodosi_android PRIVATE kodosi_runtime ghostty_vt log)
