set_source_files_properties(
    qml/Theme/KodosiTheme.qml
    qml/Theme/Identity.qml
    qml/Theme/Mentions.qml
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE
)
set_source_files_properties(
    qml/Settings/GeneralSettings.qml
    qml/Settings/DeviceSettings.qml
    qml/Settings/TerminalSettings.qml
    qml/Settings/ProviderSettings.qml
    PROPERTIES QT_QML_INTERNAL_TYPE TRUE
)

qt_add_library(kodosi-runtime STATIC
    runtime/RuntimeBridge.cpp
    runtime/RuntimeBridge.hpp
    runtime/JsonEnvelope.hpp
)

target_include_directories(kodosi-runtime
    PUBLIC
        "${CMAKE_CURRENT_SOURCE_DIR}"
)

target_link_libraries(kodosi-runtime
    PUBLIC
        Qt6::Core
    PRIVATE
        Kodosi::RustRuntime
)

qt_add_library(kodosi-application STATIC
    app/ApplicationLifecycleModel.cpp
    app/ApplicationLifecycleModel.hpp
    app/DeepLinkRouter.cpp
    app/DeepLinkRouter.hpp
    app/SingleInstanceGuard.cpp
    app/SingleInstanceGuard.hpp
    app/RuntimeStorageBootstrap.cpp
    app/RuntimeStorageBootstrap.hpp
    logging/ApplicationLogStore.cpp
    logging/ApplicationLogStore.hpp
    presentation/AccountModel.cpp
    presentation/AccountModel.hpp
    presentation/ConversationHistoryModel.cpp
    presentation/ConversationHistoryModel.hpp
    presentation/DevicesModel.cpp
    presentation/DevicesModel.hpp
    presentation/MissionsModel.cpp
    presentation/MissionsModel.hpp
    presentation/PeopleModel.cpp
    presentation/PeopleModel.hpp
    presentation/ProviderFilesModel.cpp
    presentation/ProviderFilesModel.hpp
    presentation/SessionActionsModel.cpp
    presentation/SessionActionsModel.hpp
    presentation/Workspace.cpp
    presentation/Workspace.hpp
    presentation/AppearanceModel.cpp
    presentation/AppearanceModel.hpp
    presentation/DesktopSettings.cpp
    presentation/DesktopSettings.hpp
    presentation/DesktopStateModel.cpp
    presentation/DesktopStateModel.hpp
    presentation/SessionCatalogModel.cpp
    presentation/SessionCatalogModel.hpp
    presentation/TerminalTilingLayoutModel.cpp
    presentation/TerminalTilingLayoutModel.hpp
)

target_include_directories(kodosi-application
    PUBLIC
        "${CMAKE_CURRENT_SOURCE_DIR}"
)

target_link_libraries(kodosi-application
    PUBLIC
        Qt6::Core
        kodosi-runtime
    PRIVATE
        Qt6::DBus
        Qt6::Gui
        Qt6::Network
)

qt_add_library(kodosi-desktop-integration STATIC
    platform/TerminalNotifications.cpp
    platform/TerminalNotifications.hpp
    platform/DesktopNotificationDriver.hpp
    platform/DesktopFileIntegration.cpp
    platform/DesktopFileIntegration.hpp
    platform/FreedesktopNotificationDriver.cpp
    platform/FreedesktopNotificationDriver.hpp
    platform/NotificationText.cpp
    platform/NotificationText.hpp
)

target_include_directories(kodosi-desktop-integration
    PUBLIC
        "${CMAKE_CURRENT_SOURCE_DIR}"
)

target_link_libraries(kodosi-desktop-integration
    PUBLIC
        Qt6::Core
        Qt6::DBus
        kodosi-application
    PRIVATE
        Qt6::Gui
        Qt6::Widgets
)

qt_add_library(kodosi-terminal-kernel STATIC
    accessibility/AccessibilityScope.cpp
    accessibility/AccessibilityScope.hpp
    terminal/GhosttyC.hpp
    terminal/TerminalAccessibility.cpp
    terminal/TerminalAccessibility.hpp
    terminal/GhosttyTerminalKernel.cpp
    terminal/GhosttyTerminalKernel.hpp
    terminal/TerminalLink.cpp
    terminal/TerminalLink.hpp
    terminal/TerminalFrame.hpp
    terminal/TerminalRasterizer.cpp
    terminal/TerminalRasterizer.hpp
    terminal/TerminalSessionRegistry.cpp
    terminal/TerminalSessionRegistry.hpp
    terminal/TerminalSettings.hpp
    terminal/TerminalSurfaceController.cpp
    terminal/TerminalSurfaceController.hpp
    terminal/TerminalView.cpp
    terminal/TerminalViewAccessibility.cpp
    terminal/TerminalViewInput.cpp
    terminal/TerminalViewRendering.cpp
    terminal/TerminalView.hpp
)

qt_add_library(kodosi-qml-models STATIC)
qt_add_qml_module(kodosi-qml-models
    URI Kodosi.Models
    VERSION 1.0
    OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/Kodosi/Models"
    DEPENDENCIES
        QtQuick
    SOURCES
        app/QmlModelTypes.cpp
        app/QmlModelTypes.hpp
)

target_include_directories(kodosi-qml-models
    PRIVATE
        "${CMAKE_CURRENT_SOURCE_DIR}/app"
)

target_link_libraries(kodosi-qml-models
    PUBLIC
        Qt6::Qml
        kodosi-desktop-integration
        kodosi-application
        kodosi-terminal-kernel
)

target_include_directories(kodosi-terminal-kernel PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
foreach(module Core Gui Qml Quick)
    get_target_property(module_includes Qt6::${module} INTERFACE_INCLUDE_DIRECTORIES)
    foreach(include_dir IN LISTS module_includes)
        if(include_dir MATCHES "/include/Qt${module}$")
            set(private_root "${include_dir}/${Qt6_VERSION}")
            if(NOT IS_DIRECTORY "${private_root}/Qt${module}")
                message(FATAL_ERROR "Qt ${module} private headers are required for accessibility")
            endif()
            target_include_directories(kodosi-terminal-kernel PRIVATE
                "${private_root}" "${private_root}/Qt${module}")
        endif()
    endforeach()
endforeach()

target_link_libraries(kodosi-terminal-kernel
    PUBLIC
        Qt6::Core
        Qt6::Gui
        Qt6::Quick
        kodosi-application
    PRIVATE
        Kodosi::RustRuntime
        kodosi-ghostty-vt
)

qt_add_executable(kodosi-qt
    app/main.cpp
)

qt_add_library(kodosi-ui STATIC)
qt_add_qml_module(kodosi-ui
    URI Kodosi
    RESOURCE_PREFIX /qt/qml
    OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/Kodosi"
    VERSION 1.0
    DEPENDENCIES
        Kodosi.Models
    QML_FILES
        qml/Main.qml
        qml/Screens/PeopleView.qml
        qml/Screens/MissionsView.qml
        qml/Screens/RoomContent.qml
        qml/Screens/RoomSkeleton.qml
        qml/Screens/RoomConversation.qml
        qml/Screens/RoomTasks.qml
        qml/Screens/RoomRepositories.qml
        qml/Settings/SettingsView.qml
        qml/Settings/GeneralSettings.qml
        qml/Settings/DeviceSettings.qml
        qml/Settings/TerminalSettings.qml
        qml/Settings/ProviderSettings.qml
        qml/Controls/AgentMark.qml
        qml/Controls/AvatarStack.qml
        qml/Controls/BreathingDot.qml
        qml/Controls/CardGrid.qml
        qml/Controls/CountBadge.qml
        qml/Controls/CursorBlock.qml
        qml/Controls/EmptyState.qml
        qml/Controls/IconTile.qml
        qml/Controls/KButton.qml
        qml/Controls/KFocusIndicator.qml
        qml/Controls/KBusyIndicator.qml
        qml/Controls/KComboBox.qml
        qml/Controls/KDialog.qml
        qml/Controls/KIcon.qml
        qml/Controls/KIconButton.qml
        qml/Controls/KMenu.qml
        qml/Controls/KMenuItem.qml
        qml/Controls/KPopover.qml
        qml/Controls/KReadOnlyText.qml
        qml/Controls/KScrollBar.qml
        qml/Controls/KScrollView.qml
        qml/Controls/KSpinBox.qml
        qml/Controls/KSwitch.qml
        qml/Controls/KTextField.qml
        qml/Controls/ListGroup.qml
        qml/Controls/ListRow.qml
        qml/Controls/PersonAvatar.qml
        qml/Controls/PlainLabel.qml
        qml/Controls/ProgressRing.qml
        qml/Controls/Raised.qml
        qml/Controls/RoomSigil.qml
        qml/Controls/SectionDock.qml
        qml/Controls/SegmentedPill.qml
        qml/Controls/ShimmerText.qml
        qml/Controls/Tag.qml
        qml/Controls/Well.qml
        qml/Controls/Wordmark.qml
        qml/Shell/CommandPalette.qml
        qml/Shell/Sidebar.qml
        qml/Shell/StartupOverlay.qml
        qml/Theme/Identity.qml
        qml/Theme/KodosiTheme.qml
        qml/Theme/Mentions.qml
        qml/Workbench/HistoryView.qml
        qml/Workbench/SessionDetails.qml
        qml/Workbench/ShareSheet.qml
        qml/Workbench/StageOverview.qml
        qml/Workbench/TerminalDivider.qml
        qml/Workbench/TerminalStage.qml
        qml/Workbench/TerminalTile.qml
    RESOURCES
        qml/assets/kodosi-logo-dark.png
        qml/assets/providers/claude-mark.svg
        qml/assets/providers/copilot-dark.svg
        qml/assets/providers/codex-dark.svg
        qml/assets/providers/cursor-dark.svg
)

target_link_libraries(kodosi-ui PUBLIC Qt6::Quick Qt6::QuickControls2 kodosi-qml-modelsplugin)

target_link_libraries(kodosi-qt
    PRIVATE
        Qt6::Core
        Qt6::Gui
        Qt6::Qml
        Qt6::Quick
        Qt6::QuickControls2
        Qt6::Widgets
        kodosi-desktop-integration
        kodosi-application
        kodosi-terminal-kernel
        kodosi-qml-modelsplugin
        kodosi-uiplugin
)

target_compile_definitions(kodosi-runtime
    PUBLIC
        QT_NO_CAST_FROM_ASCII
        QT_NO_CAST_TO_ASCII
        QT_NO_URL_CAST_FROM_STRING
)
target_compile_definitions(kodosi-application
    PUBLIC
        QT_NO_CAST_FROM_ASCII
        QT_NO_CAST_TO_ASCII
        QT_NO_URL_CAST_FROM_STRING
)
target_compile_definitions(kodosi-desktop-integration
    PUBLIC
        QT_NO_CAST_FROM_ASCII
        QT_NO_CAST_TO_ASCII
        QT_NO_URL_CAST_FROM_STRING
)
target_compile_definitions(kodosi-terminal-kernel
    PUBLIC
        QT_NO_CAST_FROM_ASCII
        QT_NO_CAST_TO_ASCII
        QT_NO_URL_CAST_FROM_STRING
)
target_compile_definitions(kodosi-qt PRIVATE QT_NO_CAST_FROM_ASCII QT_NO_CAST_TO_ASCII)
foreach(target kodosi-runtime kodosi-application kodosi-desktop-integration kodosi-terminal-kernel kodosi-qt)
    target_compile_options(${target} PRIVATE -Wall -Wextra -Wpedantic -Werror)
endforeach()

install(TARGETS kodosi-qt
    BUNDLE DESTINATION .
    RUNTIME DESTINATION lib/kodosi/bin
)

qt_generate_deploy_qml_app_script(
    TARGET kodosi-qt
    OUTPUT_SCRIPT kodosi_qt_deploy_script
    NO_TRANSLATIONS
    NO_UNSUPPORTED_PLATFORM_ERROR
)
set(kodosi_qt_private_deploy_script
    "${CMAKE_CURRENT_BINARY_DIR}/deploy-kodosi-private-runtime.cmake"
)
file(GENERATE
    OUTPUT "${kodosi_qt_private_deploy_script}"
    CONTENT
        "set(QT_DEPLOY_PLUGINS_DIR \"lib/kodosi/plugins\")\nset(QT_DEPLOY_QML_DIR \"lib/kodosi/qml\")\ninclude(\"${kodosi_qt_deploy_script}\")\n"
)
install(SCRIPT "${kodosi_qt_private_deploy_script}")
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
