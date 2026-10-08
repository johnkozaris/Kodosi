cmake_minimum_required(VERSION 4.3...4.4)

project(
    KodosiQT
    VERSION 0.1.0
    DESCRIPTION "Cross-platform Qt Quick desktop client for Kodosi"
    LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_INSTALL_BINDIR "lib/kodosi/bin" CACHE PATH "" FORCE)
set(CMAKE_INSTALL_LIBDIR "lib/kodosi/lib" CACHE PATH "" FORCE)
set(QT_DEPLOY_BIN_DIR "lib/kodosi/bin" CACHE STRING "" FORCE)
set(QT_DEPLOY_LIB_DIR "lib/kodosi/lib" CACHE STRING "" FORCE)
set(QT_DEPLOY_PLUGINS_DIR "lib/kodosi/plugins" CACHE STRING "" FORCE)
set(QT_DEPLOY_QML_DIR "lib/kodosi/qml" CACHE STRING "" FORCE)
set(CMAKE_DISABLE_FIND_PACKAGE_WrapVulkanHeaders TRUE)

find_package(
    Qt6 6.11.2
    EXACT
    REQUIRED
    COMPONENTS Core DBus Gui Network Qml Quick QuickControls2 Test Widgets
)
find_package(Threads REQUIRED)
find_package(Python3 REQUIRED COMPONENTS Interpreter)
find_program(CARGO_EXECUTABLE cargo REQUIRED)
find_program(DBUS_RUN_SESSION_EXECUTABLE dbus-run-session REQUIRED)
execute_process(
    COMMAND
        "${Python3_EXECUTABLE}"
        "${CMAKE_CURRENT_SOURCE_DIR}/scripts/release/source-date-epoch.py"
        --source-root "${CMAKE_CURRENT_SOURCE_DIR}"
    OUTPUT_VARIABLE KODOSI_SOURCE_DATE_EPOCH
    OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY
)
if(NOT KODOSI_SOURCE_DATE_EPOCH MATCHES "^[0-9]+$")
    message(FATAL_ERROR "SOURCE_DATE_EPOCH could not be derived from source HEAD")
endif()

set(
    KODOSI_RUNTIME_ROOT
    "${CMAKE_CURRENT_SOURCE_DIR}/../.."
)
set(
    KODOSI_GHOSTTY_ROOT
    "${CMAKE_CURRENT_SOURCE_DIR}/../../terminal/ghostty"
)
set(KODOSI_RUNTIME_MANIFEST "${KODOSI_RUNTIME_ROOT}/runtime/Cargo.toml")
set(KODOSI_RUNTIME_BUNDLE_ROOT "${CMAKE_CURRENT_BINARY_DIR}/rust-ffi")
set(KODOSI_RUST_TARGET "x86_64-unknown-linux-gnu")
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(KODOSI_RUST_PROFILE "release")
else()
    set(KODOSI_RUST_PROFILE "dev")
endif()
set(KODOSI_RUNTIME_BUNDLE
    "${KODOSI_RUNTIME_BUNDLE_ROOT}/${KODOSI_RUST_TARGET}/${KODOSI_RUST_PROFILE}/current")
set(KODOSI_RUNTIME_HEADER_DIR "${KODOSI_RUNTIME_BUNDLE}/include")
set(KODOSI_RUNTIME_ARCHIVE "${KODOSI_RUNTIME_BUNDLE}/libkodosi_ffi_c.a")
set(
    KODOSI_RUNTIME_CLI
    "${KODOSI_RUNTIME_ROOT}/runtime/target/${KODOSI_RUST_TARGET}/release/kodosi"
)
file(MAKE_DIRECTORY "${KODOSI_RUNTIME_BUNDLE_ROOT}")
set(
    KODOSI_GHOSTTY_VT_ARCHIVE
    "${KODOSI_GHOSTTY_ROOT}/Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
)
set(KODOSI_NATIVE_LICENSE_MANIFEST
    "${CMAKE_CURRENT_SOURCE_DIR}/packaging/licenses/native-license-evidence.json"
)
set(KODOSI_QT_SBOM_DIR "${CMAKE_PREFIX_PATH}/sbom")

if(NOT EXISTS "${KODOSI_RUNTIME_MANIFEST}")
    message(FATAL_ERROR "Kodosi runtime manifest not found: ${KODOSI_RUNTIME_MANIFEST}")
endif()
if(NOT EXISTS "${KODOSI_GHOSTTY_ROOT}/LinuxGhostty.ref")
    message(FATAL_ERROR "Verified kodosi-ghostty checkout not found: ${KODOSI_GHOSTTY_ROOT}")
endif()

set(KODOSI_VERIFY_GHOSTTY_COMMAND
    bash "${KODOSI_RUNTIME_ROOT}/scripts/verify-ghostty.sh"
)
execute_process(
    COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
    OUTPUT_QUIET
    COMMAND_ERROR_IS_FATAL ANY
)

add_custom_target(kodosi-rust-ffi-build
    COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
    COMMAND
        "${CMAKE_COMMAND}" -E env
        "KODOSI_GHOSTTY_DIR=${KODOSI_GHOSTTY_ROOT}"
        "SOURCE_DATE_EPOCH=${KODOSI_SOURCE_DATE_EPOCH}"
        "${Python3_EXECUTABLE}" "${KODOSI_RUNTIME_ROOT}/scripts/build-ffi-artifact.py"
        --manifest "${KODOSI_RUNTIME_MANIFEST}"
        --output-dir "${KODOSI_RUNTIME_BUNDLE_ROOT}"
        --target "${KODOSI_RUST_TARGET}"
        --profile "${KODOSI_RUST_PROFILE}"
    COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
    BYPRODUCTS
        "${KODOSI_RUNTIME_ARCHIVE}"
        "${KODOSI_RUNTIME_HEADER_DIR}/kodosi_runtime.h"
    COMMENT "Building the Kodosi ABI-7 runtime"
    WORKING_DIRECTORY "${KODOSI_RUNTIME_ROOT}"
    VERBATIM
)

add_library(kodosi-rust-runtime STATIC IMPORTED GLOBAL)
add_library(Kodosi::RustRuntime ALIAS kodosi-rust-runtime)
add_library(kodosi-ghostty-vt STATIC IMPORTED GLOBAL)
set_target_properties(kodosi-ghostty-vt PROPERTIES
    IMPORTED_LOCATION "${KODOSI_GHOSTTY_VT_ARCHIVE}"
    INTERFACE_INCLUDE_DIRECTORIES
        "${KODOSI_GHOSTTY_ROOT}/Vendor/GhosttyVt/linux-x86_64/include"
)
set_target_properties(kodosi-rust-runtime PROPERTIES
    IMPORTED_LOCATION "${KODOSI_RUNTIME_ARCHIVE}"
)
include_directories("${KODOSI_RUNTIME_HEADER_DIR}")
set_property(TARGET kodosi-rust-runtime PROPERTY INTERFACE_LINK_LIBRARIES
    "kodosi-ghostty-vt;Threads::Threads;${CMAKE_DL_LIBS};util;rt;m"
)
add_dependencies(kodosi-rust-runtime kodosi-rust-ffi-build)

if(CMAKE_BUILD_TYPE STREQUAL "Release")
    add_custom_target(kodosi-native-license-evidence ALL
        COMMAND
            "${Python3_EXECUTABLE}"
            "${CMAKE_CURRENT_SOURCE_DIR}/scripts/release/verify-native-license-evidence.py"
            --manifest "${KODOSI_NATIVE_LICENSE_MANIFEST}"
            --license-root "${CMAKE_CURRENT_SOURCE_DIR}/packaging/licenses"
            --qt-root "${CMAKE_PREFIX_PATH}"
            --dependencies "${CMAKE_CURRENT_SOURCE_DIR}/dependencies.lock.json"
        COMMENT "Verifying Qt 6.11.2 and ICU 73.2 license/SBOM evidence"
        VERBATIM
    )

    add_custom_target(kodosi-rust-cli-build ALL
        COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
        COMMAND
            "${CMAKE_COMMAND}" -E env
            "KODOSI_GHOSTTY_DIR=${KODOSI_GHOSTTY_ROOT}"
            "SOURCE_DATE_EPOCH=${KODOSI_SOURCE_DATE_EPOCH}"
            "${CARGO_EXECUTABLE}" build
            --manifest-path "${KODOSI_RUNTIME_MANIFEST}"
            --locked
            --target "${KODOSI_RUST_TARGET}"
            --release
            -p kodosi-runtime
            --bin kodosi
        BYPRODUCTS "${KODOSI_RUNTIME_CLI}"
        COMMENT "Building the pinned Kodosi CLI"
        WORKING_DIRECTORY "${KODOSI_RUNTIME_ROOT}"
        VERBATIM
    )
    add_dependencies(kodosi-rust-cli-build kodosi-rust-ffi-build)

    set(KODOSI_SOURCE_IDENTITY
        "${CMAKE_CURRENT_BINARY_DIR}/generated/source-identity.json"
    )
    add_custom_target(kodosi-source-identity ALL
        COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
        COMMAND
            "${Python3_EXECUTABLE}"
            "${CMAKE_CURRENT_SOURCE_DIR}/scripts/release/generate-source-identity.py"
            --source-root "${CMAKE_CURRENT_SOURCE_DIR}"
                    --output "${KODOSI_SOURCE_IDENTITY}"
        BYPRODUCTS "${KODOSI_SOURCE_IDENTITY}"
        COMMENT "Capturing the Kodosi package source identity"
        VERBATIM
    )

    set(KODOSI_RUST_LICENSE_DIR
        "${CMAKE_CURRENT_BINARY_DIR}/generated/rust-dependency-licenses"
    )
    add_custom_target(kodosi-rust-license-inventory ALL
        COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
        COMMAND
            "${Python3_EXECUTABLE}"
            "${CMAKE_CURRENT_SOURCE_DIR}/scripts/release/generate-rust-license-inventory.py"
            --manifest "${KODOSI_RUNTIME_MANIFEST}"
            --output-dir "${KODOSI_RUST_LICENSE_DIR}"
            --overrides
            "${CMAKE_CURRENT_SOURCE_DIR}/packaging/licenses/rust-license-overrides.json"
        BYPRODUCTS
            "${KODOSI_RUST_LICENSE_DIR}/rust-dependency-licenses.json"
        COMMENT "Generating the locked Linux Rust dependency license inventory"
        VERBATIM
    )
    add_dependencies(
        kodosi-rust-license-inventory
        kodosi-rust-cli-build
    )
endif()

qt_standard_project_setup(
    REQUIRES 6.11.2
    I18N_TRANSLATED_LANGUAGES
)

include(CTest)
add_subdirectory(src)

install(
    FILES packaging/linux/com.kodosi.Kodosi.desktop
    DESTINATION share/applications
)
install(
    FILES packaging/linux/com.kodosi.Kodosi.metainfo.xml
    DESTINATION share/metainfo
)
install(
    FILES packaging/linux/com.kodosi.Kodosi.svg
    DESTINATION share/icons/hicolor/scalable/apps
)
install(
    FILES packaging/linux/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt
    DESTINATION share/doc/kodosi
)
install(
    FILES dependencies.lock.json
    DESTINATION share/doc/kodosi/provenance
)
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    install(CODE "
        execute_process(
            COMMAND
                \"${Python3_EXECUTABLE}\"
                \"${CMAKE_CURRENT_SOURCE_DIR}/scripts/release/verify-release-sources.py\"
                --source-root
                \"${CMAKE_CURRENT_SOURCE_DIR}\"
            RESULT_VARIABLE KODOSI_INSTALL_SOURCE_RESULT
            ERROR_VARIABLE KODOSI_INSTALL_SOURCE_ERROR
        )
        if(NOT KODOSI_INSTALL_SOURCE_RESULT EQUAL 0)
            message(FATAL_ERROR
                \"Source verification immediately before install failed: \${KODOSI_INSTALL_SOURCE_ERROR}\"
            )
        endif()
    ")
    install(
        FILES "${KODOSI_SOURCE_IDENTITY}"
        DESTINATION share/doc/kodosi/provenance
    )
    install(
        DIRECTORY "${KODOSI_RUST_LICENSE_DIR}/"
        DESTINATION share/doc/kodosi/rust
    )
    install(
        DIRECTORY packaging/licenses/qt/6.11.2/
        DESTINATION share/doc/kodosi/qt
    )
    install(
        DIRECTORY packaging/licenses/icu/73.2/
        DESTINATION share/doc/kodosi/icu
    )
    install(
        FILES "${KODOSI_NATIVE_LICENSE_MANIFEST}"
        DESTINATION share/doc/kodosi/provenance
    )
    install(
        FILES
            "${KODOSI_QT_SBOM_DIR}/qtbase-6.11.2.spdx"
            "${KODOSI_QT_SBOM_DIR}/qtbase-6.11.2.spdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtbase-6.11.2.cdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtdeclarative-6.11.2.spdx"
            "${KODOSI_QT_SBOM_DIR}/qtdeclarative-6.11.2.spdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtdeclarative-6.11.2.cdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtsvg-6.11.2.spdx"
            "${KODOSI_QT_SBOM_DIR}/qtsvg-6.11.2.spdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtsvg-6.11.2.cdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtwayland-6.11.2.spdx"
            "${KODOSI_QT_SBOM_DIR}/qtwayland-6.11.2.spdx.json"
            "${KODOSI_QT_SBOM_DIR}/qtwayland-6.11.2.cdx.json"
        DESTINATION share/doc/kodosi/qt/sbom
    )
endif()
install(
    FILES
        "${KODOSI_GHOSTTY_ROOT}/LICENSE"
        "${KODOSI_GHOSTTY_ROOT}/LICENSE-GHOSTTY"
        "${KODOSI_GHOSTTY_ROOT}/LinuxGhostty.ref"
        "${KODOSI_GHOSTTY_ROOT}/THIRD_PARTY_NOTICES.md"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/linux-vt-inventory.json"
    DESTINATION share/doc/kodosi/ghostty
)
install(
    FILES
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/highway-Apache-2.0.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/highway-BSD-3-Clause.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/simdutf-MIT.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/simdutf-PyTorch-BSD-3-Clause.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/simdutf-Fuchsia-BSD-3-Clause.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/uucode.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/uucode-Bjoern-Hoehrmann.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/unicode-data.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/wuffs.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/zig-compiler-runtime.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/compiler-rt-LLVM-derived.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/zig-runtime-Apache-2.0-LLVM-exception.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/compiler-rt-musl-derived.txt"
        "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/zig-runtime-MIT-derived.txt"
    DESTINATION share/doc/kodosi/ghostty/ThirdPartyNotices/licenses
)
install(
    PROGRAMS
        packaging/linux/kodosi
        packaging/linux/kodosi-qt
    DESTINATION bin
)
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    install(
        PROGRAMS "${KODOSI_RUNTIME_CLI}"
        DESTINATION lib/kodosi/bin
    )
endif()

if(BUILD_TESTING)
    add_subdirectory(tests)
endif()

set(CPACK_PACKAGE_NAME "kodosi")
set(CPACK_PACKAGE_VENDOR "Kodosi")
set(CPACK_PACKAGE_CONTACT "support@kodosi.com")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY
    "Local terminals with trusted device and friend access"
)
set(CPACK_PACKAGE_VERSION "${PROJECT_VERSION}")
set(CPACK_PACKAGING_INSTALL_PREFIX "/usr")
set(CPACK_GENERATOR "TGZ;DEB")
set(CPACK_DEBIAN_FILE_NAME DEB-DEFAULT)
set(CPACK_DEBIAN_PACKAGE_SECTION "devel")
set(CPACK_DEBIAN_PACKAGE_PRIORITY "optional")
set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS OFF)
set(CPACK_DEBIAN_PACKAGE_DEPENDS
    "libc6 (>= 2.39), libstdc++6, libgcc-s1, desktop-file-utils, libatk1.0-0t64, libbrotli1, libcairo-gobject2, libcairo2, libdbus-1-3, libdrm2, libegl1, libfontconfig1, libfreetype6, libgbm1, libgdk-pixbuf-2.0-0, libgl1, libglib2.0-0t64, libgssapi-krb5-2, libgtk-3-0t64, libharfbuzz0b, libopengl0, libpango-1.0-0, libpangocairo-1.0-0, libwayland-client0, libwayland-cursor0, libwayland-egl1, libx11-6, libx11-xcb1, libxcb-cursor0, libxcb-glx0, libxcb-icccm4, libxcb-image0, libxcb-keysyms1, libxcb-randr0, libxcb-render-util0, libxcb-render0, libxcb-shape0, libxcb-shm0, libxcb-sync1, libxcb-util1, libxcb-xfixes0, libxcb-xkb1, libxcb1, libxkbcommon-x11-0, libxkbcommon0, libzstd1, xdg-desktop-portal, xdg-utils, zlib1g"
)
set(CPACK_DEBIAN_PACKAGE_HOMEPAGE "https://kodosi.com")
set(CPACK_ARCHIVE_FILE_NAME
    "kodosi-${PROJECT_VERSION}-linux-x86_64"
)
set(CPACK_THREADS 1)
include(CPack)

install(DIRECTORY packaging/licenses/provider-icons/ DESTINATION share/licenses/kodosi-qt/provider-icons)
