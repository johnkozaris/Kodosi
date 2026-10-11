# Linux: a private runtime under lib/kodosi, and the Debian and portable
# packages with their license evidence.

set(CMAKE_INSTALL_BINDIR "lib/kodosi/bin" CACHE PATH "" FORCE)
set(CMAKE_INSTALL_LIBDIR "lib/kodosi/lib" CACHE PATH "" FORCE)
set(QT_DEPLOY_BIN_DIR "lib/kodosi/bin" CACHE STRING "" FORCE)
set(QT_DEPLOY_LIB_DIR "lib/kodosi/lib" CACHE STRING "" FORCE)
set(QT_DEPLOY_PLUGINS_DIR "lib/kodosi/plugins" CACHE STRING "" FORCE)
set(QT_DEPLOY_QML_DIR "lib/kodosi/qml" CACHE STRING "" FORCE)

set(KODOSI_PLATFORM_QT_COMPONENTS DBus)
set(KODOSI_RUST_TARGET "x86_64-unknown-linux-gnu")
set(KODOSI_RUNTIME_ARCHIVE_NAME "libkodosi_ffi_c.a")
set(KODOSI_RUNTIME_CLI_NAME "kodosi")
set(KODOSI_GHOSTTY_REF "LinuxGhostty.ref")
set(KODOSI_GHOSTTY_LAYOUT "linux-x86_64")
set(KODOSI_GHOSTTY_VT_ARCHIVE_NAME "libghostty-vt.a")
set(KODOSI_RUNTIME_SYSTEM_LIBRARIES Threads::Threads ${CMAKE_DL_LIBS} util rt m)
set(KODOSI_COMPILE_OPTIONS -Wall -Wextra -Wpedantic -Werror)
set(KODOSI_PLATFORM_SOURCES
    platform/linux/FreedesktopNotificationDriver.cpp
    platform/linux/FreedesktopNotificationDriver.hpp
)
set(KODOSI_PLATFORM_LIBRARIES Qt6::DBus)
set(KODOSI_QT_SBOM_MODULES qtbase qtdeclarative qtsvg qtwayland)
set(KODOSI_NATIVE_LICENSE_MANIFEST "${PROJECT_SOURCE_DIR}/packaging/licenses/native-license-evidence.json")

find_program(DBUS_RUN_SESSION_EXECUTABLE dbus-run-session REQUIRED)
set(KODOSI_TEST_LAUNCHER "${DBUS_RUN_SESSION_EXECUTABLE}" --)
set(KODOSI_TEST_ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
set(KODOSI_TEST_ENVIRONMENT_MODIFICATION "")

# Installs the app, its private Qt runtime, and the package's notices and
# evidence. Called from the directory that defines the app target.
function(kodosi_install app)
    install(TARGETS ${app}
        BUNDLE DESTINATION .
        RUNTIME DESTINATION lib/kodosi/bin
    )
    qt_generate_deploy_qml_app_script(
        TARGET ${app}
        OUTPUT_SCRIPT deploy_script
        NO_TRANSLATIONS
        NO_UNSUPPORTED_PLATFORM_ERROR
    )
    set(private_deploy_script "${CMAKE_CURRENT_BINARY_DIR}/deploy-kodosi-private-runtime.cmake")
    file(GENERATE
        OUTPUT "${private_deploy_script}"
        CONTENT
            "set(QT_DEPLOY_PLUGINS_DIR \"lib/kodosi/plugins\")\nset(QT_DEPLOY_QML_DIR \"lib/kodosi/qml\")\ninclude(\"${deploy_script}\")\n"
    )
    install(SCRIPT "${private_deploy_script}")
    install(
        FILES
            "$<TARGET_FILE:Qt6::QOffscreenIntegrationPlugin>"
            "$<TARGET_FILE:Qt6::QWaylandIntegrationPlugin>"
        DESTINATION lib/kodosi/plugins/platforms
    )
    install(
        FILES
            "$<TARGET_FILE:Qt6::QGtk3ThemePlugin>"
            "$<TARGET_FILE:Qt6::QXdgDesktopPortalThemePlugin>"
        DESTINATION lib/kodosi/plugins/platformthemes
    )
    install(
        FILES "$<TARGET_FILE:Qt6::QWaylandEglClientBufferPlugin>"
        DESTINATION lib/kodosi/plugins/wayland-graphics-integration-client
    )
    install(
        FILES
            "$<TARGET_FILE:Qt6::QWaylandWlShellIntegrationPlugin>"
            "$<TARGET_FILE:Qt6::QWaylandXdgShellIntegrationPlugin>"
        DESTINATION lib/kodosi/plugins/wayland-shell-integration
    )

    install(
        FILES "${PROJECT_SOURCE_DIR}/packaging/linux/com.kodosi.Kodosi.desktop"
        DESTINATION share/applications
    )
    install(
        FILES "${PROJECT_SOURCE_DIR}/packaging/linux/com.kodosi.Kodosi.metainfo.xml"
        DESTINATION share/metainfo
    )
    install(
        FILES "${PROJECT_SOURCE_DIR}/packaging/linux/com.kodosi.Kodosi.svg"
        DESTINATION share/icons/hicolor/scalable/apps
    )
    install(
        FILES "${PROJECT_SOURCE_DIR}/packaging/linux/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt"
        DESTINATION share/doc/kodosi
    )
    kodosi_install_evidence(share/doc/kodosi)
    install(
        FILES "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/linux-vt-inventory.json"
        DESTINATION share/doc/kodosi/ghostty
    )
    if(CMAKE_BUILD_TYPE STREQUAL "Release")
        add_custom_target(kodosi-native-license-evidence ALL
            COMMAND
                "${Python3_EXECUTABLE}"
                "${PROJECT_SOURCE_DIR}/scripts/release/verify-native-license-evidence.py"
                --manifest "${KODOSI_NATIVE_LICENSE_MANIFEST}"
                --license-root "${PROJECT_SOURCE_DIR}/packaging/licenses"
                --qt-root "${QT6_INSTALL_PREFIX}"
                --dependencies "${PROJECT_SOURCE_DIR}/dependencies.lock.json"
            COMMENT "Verifying Qt ${KODOSI_QT_VERSION} and ICU 73.2 license/SBOM evidence"
            VERBATIM
        )
        install(
            DIRECTORY "${PROJECT_SOURCE_DIR}/packaging/licenses/icu/73.2/"
            DESTINATION share/doc/kodosi/icu
        )
        install(
            FILES "${KODOSI_NATIVE_LICENSE_MANIFEST}"
            DESTINATION share/doc/kodosi/provenance
        )
    endif()
    install(
        PROGRAMS
            "${PROJECT_SOURCE_DIR}/packaging/linux/kodosi"
            "${PROJECT_SOURCE_DIR}/packaging/linux/kodosi-qt"
        DESTINATION bin
    )
    if(CMAKE_BUILD_TYPE STREQUAL "Release")
        install(
            PROGRAMS "${KODOSI_RUNTIME_CLI}"
            DESTINATION lib/kodosi/bin
        )
    endif()
endfunction()
