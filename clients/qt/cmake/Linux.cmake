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
set(KODOSI_AGENT_BROWSER_NAME "agent-browser")
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
set(KODOSI_PROVIDER_ICON_LICENSE_DIR share/licenses/kodosi-qt/provider-icons)
set(KODOSI_NATIVE_LICENSE_MANIFEST "${PROJECT_SOURCE_DIR}/packaging/licenses/native-license-evidence.json")

find_program(DBUS_RUN_SESSION_EXECUTABLE dbus-run-session REQUIRED)
set(KODOSI_TEST_LAUNCHER "${DBUS_RUN_SESSION_EXECUTABLE}" --)
set(KODOSI_TEST_ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
set(KODOSI_TEST_ENVIRONMENT_MODIFICATION "")

function(kodosi_install app)
    install(TARGETS ${app} RUNTIME DESTINATION lib/kodosi/bin)
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
        DIRECTORY "${PROJECT_SOURCE_DIR}/packaging/linux/icons/"
        DESTINATION share/icons/hicolor
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
            PROGRAMS "${KODOSI_RUNTIME_CLI}" "${KODOSI_AGENT_BROWSER}"
            DESTINATION lib/kodosi/bin
        )
        install(
            DIRECTORY "${KODOSI_AGENT_BROWSER_DIR}/notices/"
            DESTINATION share/doc/kodosi/agent-browser
        )
    endif()
endfunction()

set(CPACK_PACKAGE_NAME "kodosi")
set(CPACK_PACKAGE_VENDOR "Kodosi")
set(CPACK_PACKAGE_CONTACT "support@kodosi.com")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY
    "Shared terminals and rooms for people and their coding agents"
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
set(CPACK_KODOSI_LINUX_PACKAGING "${PROJECT_SOURCE_DIR}/packaging/linux")
set(CPACK_PRE_BUILD_SCRIPTS "${PROJECT_SOURCE_DIR}/cmake/AptRepository.cmake")
set(CPACK_DEBIAN_PACKAGE_CONTROL_EXTRA "${PROJECT_SOURCE_DIR}/packaging/linux/conffiles")
set(CPACK_DEBIAN_PACKAGE_CONTROL_STRICT_PERMISSION ON)
include(CPack)
