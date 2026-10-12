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
set(CMAKE_DISABLE_FIND_PACKAGE_WrapVulkanHeaders TRUE)
file(READ "${CMAKE_CURRENT_SOURCE_DIR}/dependencies.lock.json" KODOSI_DEPENDENCIES)
string(JSON KODOSI_QT_VERSION GET "${KODOSI_DEPENDENCIES}" frameworks qt version)
set_property(
    DIRECTORY
    APPEND
    PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/dependencies.lock.json"
)
set(KODOSI_RUNTIME_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
set(KODOSI_GHOSTTY_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../terminal/ghostty")
include(cmake/${CMAKE_SYSTEM_NAME}.cmake)
include(cmake/Evidence.cmake)

find_package(
    Qt6 ${KODOSI_QT_VERSION}
    EXACT
    REQUIRED
    COMPONENTS Core Gui Qml Quick QuickControls2 Test Widgets ${KODOSI_PLATFORM_QT_COMPONENTS}
)
find_package(Threads REQUIRED)
find_package(Python3 REQUIRED COMPONENTS Interpreter)
find_program(CARGO_EXECUTABLE cargo REQUIRED)
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

set(KODOSI_RUNTIME_MANIFEST "${KODOSI_RUNTIME_ROOT}/runtime/Cargo.toml")
set(KODOSI_RUNTIME_BUNDLE_ROOT "${CMAKE_CURRENT_BINARY_DIR}/rust-ffi")
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(KODOSI_RUST_PROFILE "release")
else()
    set(KODOSI_RUST_PROFILE "dev")
endif()
set(KODOSI_RUNTIME_BUNDLE
    "${KODOSI_RUNTIME_BUNDLE_ROOT}/${KODOSI_RUST_TARGET}/${KODOSI_RUST_PROFILE}")
set(KODOSI_RUNTIME_HEADER_DIR "${KODOSI_RUNTIME_BUNDLE}/include")
set(KODOSI_RUNTIME_ARCHIVE "${KODOSI_RUNTIME_BUNDLE}/${KODOSI_RUNTIME_ARCHIVE_NAME}")
set(
    KODOSI_RUNTIME_CLI
    "${KODOSI_RUNTIME_ROOT}/runtime/target/${KODOSI_RUST_TARGET}/release/${KODOSI_RUNTIME_CLI_NAME}"
)
file(MAKE_DIRECTORY "${KODOSI_RUNTIME_BUNDLE_ROOT}")
set(KODOSI_GHOSTTY_VT_ROOT "${KODOSI_GHOSTTY_ROOT}/Vendor/GhosttyVt/${KODOSI_GHOSTTY_LAYOUT}")

if(NOT EXISTS "${KODOSI_RUNTIME_MANIFEST}")
    message(FATAL_ERROR "Kodosi runtime manifest not found: ${KODOSI_RUNTIME_MANIFEST}")
endif()
if(NOT EXISTS "${KODOSI_GHOSTTY_ROOT}/${KODOSI_GHOSTTY_REF}")
    message(FATAL_ERROR "Ghostty integration not found: ${KODOSI_GHOSTTY_ROOT}")
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
    COMMENT "Building the Kodosi ABI-8 runtime"
    WORKING_DIRECTORY "${KODOSI_RUNTIME_ROOT}"
    VERBATIM
)

add_library(kodosi-rust-runtime STATIC IMPORTED GLOBAL)
add_library(Kodosi::RustRuntime ALIAS kodosi-rust-runtime)
add_library(kodosi-ghostty-vt STATIC IMPORTED GLOBAL)
set_target_properties(kodosi-ghostty-vt PROPERTIES
    IMPORTED_LOCATION "${KODOSI_GHOSTTY_VT_ROOT}/lib/${KODOSI_GHOSTTY_VT_ARCHIVE_NAME}"
    INTERFACE_INCLUDE_DIRECTORIES "${KODOSI_GHOSTTY_VT_ROOT}/include"
    INTERFACE_COMPILE_DEFINITIONS GHOSTTY_STATIC
)
set_target_properties(kodosi-rust-runtime PROPERTIES
    IMPORTED_LOCATION "${KODOSI_RUNTIME_ARCHIVE}"
)
include_directories("${KODOSI_RUNTIME_HEADER_DIR}")
set_property(TARGET kodosi-rust-runtime PROPERTY INTERFACE_LINK_LIBRARIES
    "kodosi-ghostty-vt;${KODOSI_RUNTIME_SYSTEM_LIBRARIES}"
)
add_dependencies(kodosi-rust-runtime kodosi-rust-ffi-build)

if(CMAKE_BUILD_TYPE STREQUAL "Release")
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
endif()

qt_standard_project_setup(
    REQUIRES ${KODOSI_QT_VERSION}
    I18N_TRANSLATED_LANGUAGES
)

include(CTest)
add_subdirectory(src)

if(BUILD_TESTING)
    add_subdirectory(tests)
endif()
