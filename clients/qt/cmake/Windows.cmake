# Windows: clang-cl against Qt's MSVC build. The app, its Qt runtime, ConPTY
# and the CLI sit side by side, as the MSIX package lays them out.

if(NOT (CMAKE_CXX_COMPILER_ID STREQUAL "Clang" AND CMAKE_CXX_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC"))
    message(FATAL_ERROR "Build the Windows client with clang-cl")
endif()

if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(ARM64|aarch64)$")
    set(KODOSI_RUST_TARGET "aarch64-pc-windows-msvc")
    set(KODOSI_GHOSTTY_LAYOUT "windows-arm64")
    set(KODOSI_CONPTY_ARCH "arm64")
else()
    set(KODOSI_RUST_TARGET "x86_64-pc-windows-msvc")
    set(KODOSI_GHOSTTY_LAYOUT "windows-x86_64")
    set(KODOSI_CONPTY_ARCH "x64")
endif()

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
set(KODOSI_CONPTY_DIR "${PROJECT_BINARY_DIR}/conpty")

# Places ConPTY beside the app so it runs from the build tree, and installs
# the app with its Qt runtime and the CLI. Called from the directory that
# defines the app target.
function(kodosi_install app)
    set_target_properties(${app} PROPERTIES WIN32_EXECUTABLE ON)
    execute_process(
        COMMAND
            "${Python3_EXECUTABLE}" "${KODOSI_RUNTIME_ROOT}/terminal/conpty/fetch.py"
            --arch "${KODOSI_CONPTY_ARCH}"
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
    qt_generate_deploy_qml_app_script(
        TARGET ${app}
        OUTPUT_SCRIPT deploy_script
        NO_TRANSLATIONS
    )
    install(SCRIPT "${deploy_script}")
    if(CMAKE_BUILD_TYPE STREQUAL "Release")
        install(PROGRAMS "${KODOSI_RUNTIME_CLI}" DESTINATION .)
    endif()
endfunction()
