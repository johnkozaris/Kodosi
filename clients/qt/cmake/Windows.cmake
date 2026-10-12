# Windows: clang-cl against Qt's MSVC build. The install tree is the MSIX
# package layout: the app, the CLI, ConPTY, the C runtime and the Qt runtime
# side by side, with the package manifest, its assets and the notices.

if(NOT (CMAKE_CXX_COMPILER_ID STREQUAL "Clang" AND CMAKE_CXX_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC"))
    message(FATAL_ERROR "Build the Windows client with clang-cl")
endif()

if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(ARM64|aarch64)$")
    set(KODOSI_RUST_TARGET "aarch64-pc-windows-msvc")
    set(KODOSI_GHOSTTY_LAYOUT "windows-arm64")
    set(KODOSI_WINDOWS_ARCH "arm64")
else()
    set(KODOSI_RUST_TARGET "x86_64-pc-windows-msvc")
    set(KODOSI_GHOSTTY_LAYOUT "windows-x86_64")
    set(KODOSI_WINDOWS_ARCH "x64")
endif()

# Rust links the release C runtime in every profile, so every configuration
# does, with Qt's release libraries. The empty entry keeps the Rust runtime and
# Ghostty imports, which have no configurations.
set(CMAKE_MSVC_RUNTIME_LIBRARY MultiThreadedDLL)
set(CMAKE_MAP_IMPORTED_CONFIG_DEBUG RelWithDebInfo "")

set(QT_DEPLOY_BIN_DIR "." CACHE STRING "" FORCE)
set(KODOSI_PLATFORM_QT_COMPONENTS "")
set(KODOSI_RUNTIME_ARCHIVE_NAME "kodosi_ffi_c.lib")
set(KODOSI_RUNTIME_CLI_NAME "kodosi.exe")
set(KODOSI_GHOSTTY_REF "WindowsGhostty.ref")
set(KODOSI_GHOSTTY_VT_ARCHIVE_NAME "ghostty-vt-static.lib")
# rustc's native-static-libs for kodosi-ffi-c. The C runtime is the dynamic
# one Qt uses.
set(KODOSI_RUNTIME_SYSTEM_LIBRARIES kernel32 ntdll userenv ws2_32 dbghelp)
set(KODOSI_COMPILE_OPTIONS /W4 -Wpedantic /WX)
set(KODOSI_PLATFORM_SOURCES
    platform/windows/ToastNotificationDriver.cpp
    platform/windows/ToastNotificationDriver.hpp
)
set(KODOSI_PLATFORM_LIBRARIES windowsapp)
set(KODOSI_TEST_LAUNCHER "")
set(KODOSI_TEST_ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
# Tests run from the build tree, so they find Qt's DLLs where Qt keeps them.
set(KODOSI_TEST_ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
set(KODOSI_QT_SBOM_MODULES qtbase qtdeclarative qtsvg)
set(KODOSI_PROVIDER_ICON_LICENSE_DIR doc/provider-icons)
set(KODOSI_CONPTY_DIR "${PROJECT_BINARY_DIR}/conpty")
# Windows installs an unsigned package only when its publisher carries this
# OID; release builds set the signing certificate's subject instead.
set(KODOSI_MSIX_PUBLISHER "CN=Kodosi Development, OID.2.25.311729368913984317654407730594956997722=1"
    CACHE STRING "MSIX publisher, equal to the signing certificate subject")

# Places ConPTY beside the app so it runs from the build tree, and installs
# the package layout. Called from the directory that defines the app target.
function(kodosi_install app)
    set_target_properties(${app} PROPERTIES WIN32_EXECUTABLE ON)
    execute_process(
        COMMAND
            "${Python3_EXECUTABLE}" "${KODOSI_RUNTIME_ROOT}/terminal/conpty/install-conpty.py"
            --arch "${KODOSI_WINDOWS_ARCH}"
            --output "${KODOSI_CONPTY_DIR}"
        COMMAND_ERROR_IS_FATAL ANY
    )
    set(conpty "${KODOSI_CONPTY_DIR}/conpty.dll" "${KODOSI_CONPTY_DIR}/OpenConsole.exe")
    add_custom_command(TARGET ${app} POST_BUILD
        COMMAND "${CMAKE_COMMAND}" -E copy_if_different ${conpty} "$<TARGET_FILE_DIR:${app}>"
        VERBATIM
    )
    install(TARGETS ${app} RUNTIME DESTINATION .)
    install(FILES ${conpty} DESTINATION .)
    install(
        FILES "${KODOSI_RUNTIME_ROOT}/terminal/conpty/LICENSE" "${KODOSI_RUNTIME_ROOT}/terminal/conpty/ConPTY.lock"
        DESTINATION doc/conpty
    )

    # The C runtime from the toolset that built the app, rather than an older
    # system or framework copy.
    set(CMAKE_INSTALL_SYSTEM_RUNTIME_DESTINATION .)
    include(InstallRequiredSystemLibraries)

    # Windows 11 provides the Direct3D compilers; Qt Quick renders with D3D11.
    qt_generate_deploy_qml_app_script(
        TARGET ${app}
        OUTPUT_SCRIPT deploy_script
        NO_TRANSLATIONS
        NO_COMPILER_RUNTIME
        DEPLOY_TOOL_OPTIONS --no-opengl-sw --no-system-d3d-compiler --no-system-dxc-compiler
    )
    install(SCRIPT "${deploy_script}")

    set(KODOSI_MSIX_VERSION "${PROJECT_VERSION}.0")
    string(REPLACE "&" "&amp;" KODOSI_MSIX_PUBLISHER_XML "${KODOSI_MSIX_PUBLISHER}")
    string(REPLACE "\"" "&quot;" KODOSI_MSIX_PUBLISHER_XML "${KODOSI_MSIX_PUBLISHER_XML}")
    string(REPLACE "<" "&lt;" KODOSI_MSIX_PUBLISHER_XML "${KODOSI_MSIX_PUBLISHER_XML}")
    configure_file(
        "${PROJECT_SOURCE_DIR}/packaging/windows/AppxManifest.xml.in"
        "${PROJECT_BINARY_DIR}/AppxManifest.xml"
        @ONLY
    )
    install(FILES "${PROJECT_BINARY_DIR}/AppxManifest.xml" DESTINATION .)
    install(DIRECTORY "${PROJECT_SOURCE_DIR}/packaging/windows/Assets" DESTINATION .)

    kodosi_install_evidence(doc)
    if(CMAKE_BUILD_TYPE STREQUAL "Release")
        install(PROGRAMS "${KODOSI_RUNTIME_CLI}" DESTINATION .)
    endif()
endfunction()
