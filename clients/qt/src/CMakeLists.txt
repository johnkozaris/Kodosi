set_source_files_properties(
    qml/Theme/KodosiTheme.qml
    qml/Theme/Identity.qml
    qml/Theme/Mentions.qml
    qml/Dosi/Fold.qml
    qml/Controls/Tip.qml
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE
)
set_source_files_properties(
    qml/Settings/GeneralSettings.qml
    qml/Settings/DeviceSettings.qml
    qml/Settings/TerminalSettings.qml
    qml/Settings/ProviderSettings.qml
    qml/Settings/DosiSettings.qml
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
    app/AppActivation.cpp
    app/AppActivation.hpp
    app/DeepLinkRouter.cpp
    app/DeepLinkRouter.hpp
    logging/PerformanceSpan.cpp
    logging/PerformanceSpan.hpp
    logging/QtLogForwarder.cpp
    logging/QtLogForwarder.hpp
    presentation/AccountModel.cpp
    presentation/AccountModel.hpp
    presentation/AgentLauncherModel.cpp
    presentation/AgentLauncherModel.hpp
    presentation/ConversationHistoryModel.cpp
    presentation/ConversationHistoryModel.hpp
    presentation/DevicesModel.cpp
    presentation/DevicesModel.hpp
    presentation/DosiModel.cpp
    presentation/DosiModel.hpp
    presentation/DosiWords.cpp
    presentation/DosiWords.hpp
    presentation/KeyedListModel.cpp
    presentation/KeyedListModel.hpp
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
        Kodosi::RustRuntime
        Qt6::Gui
)

qt_add_library(kodosi-desktop-integration STATIC
    platform/TerminalNotifications.cpp
    platform/TerminalNotifications.hpp
    platform/DesktopNotificationDriver.hpp
    platform/DesktopFileIntegration.cpp
    platform/DesktopFileIntegration.hpp
    platform/DesktopTray.cpp
    platform/DesktopTray.hpp
    platform/NotificationText.cpp
    platform/NotificationText.hpp
    platform/SystemNotificationDriver.hpp
    ${KODOSI_PLATFORM_SOURCES}
)

target_include_directories(kodosi-desktop-integration
    PUBLIC
        "${CMAKE_CURRENT_SOURCE_DIR}"
)

target_link_libraries(kodosi-desktop-integration
    PUBLIC
        Qt6::Core
        ${KODOSI_PLATFORM_LIBRARIES}
        kodosi-application
    PRIVATE
        Qt6::Gui
        Qt6::Widgets
)

qt_add_resources(kodosi-desktop-integration "tray"
    PREFIX "/kodosi"
    BASE platform
    FILES platform/kodosi-tray.svg
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
    terminal/TerminalImages.cpp
    terminal/TerminalImages.hpp
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
            if(WIN32 AND module MATCHES "^(Core|Gui)$")
                target_include_directories(kodosi-desktop-integration PRIVATE
                    "${private_root}" "${private_root}/Qt${module}")
            endif()
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
    SOURCES
        controls/LightTrack.cpp
        controls/LightTrack.hpp
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
        qml/Settings/DosiSettings.qml
        qml/Controls/AccountMark.qml
        qml/Controls/Arrive.qml
        qml/Controls/Cue.qml
        qml/Controls/DosiFace.qml
        qml/Controls/Spinner.qml
        qml/Dosi/DosiComposer.qml
        qml/Dosi/DosiFold.qml
        qml/Dosi/DosiIsland.qml
        qml/Dosi/DosiKeys.qml
        qml/Dosi/DosiMini.qml
        qml/Dosi/DosiNewChat.qml
        qml/Dosi/DosiPanel.qml
        qml/Dosi/DosiQuestion.qml
        qml/Dosi/DosiRow.qml
        qml/Dosi/DosiShelf.qml
        qml/Dosi/DosiSteps.qml
        qml/Dosi/DosiThread.qml
        qml/Dosi/DosiTouch.qml
        qml/Dosi/Fold.qml
        qml/Dosi/FoldSpot.qml
        qml/Controls/ActivityLine.qml
        qml/Controls/AgentMark.qml
        qml/Controls/AvatarStack.qml
        qml/Controls/Bezel.qml
        qml/Controls/BreathingDot.qml
        qml/Controls/CardGrid.qml
        qml/Controls/CodeKeys.qml
        qml/Controls/CountBadge.qml
        qml/Controls/CursorBlock.qml
        qml/Controls/DashedSlot.qml
        qml/Controls/EffortBars.qml
        qml/Controls/EmptyState.qml
        qml/Controls/IconTile.qml
        qml/Controls/KButton.qml
        qml/Controls/KFocusIndicator.qml
        qml/Controls/KDialog.qml
        qml/Controls/KIcon.qml
        qml/Controls/KIconButton.qml
        qml/Controls/Squash.qml
        qml/Controls/Tip.qml
        qml/Controls/KMenu.qml
        qml/Controls/KMenuItem.qml
        qml/Controls/KPopover.qml
        qml/Controls/KReadOnlyText.qml
        qml/Controls/KScrollBar.qml
        qml/Controls/KScrollView.qml
        qml/Controls/KSpinBox.qml
        qml/Controls/KSwitch.qml
        qml/Controls/KTextField.qml
        qml/Controls/LightRim.qml
        qml/Controls/ListGroup.qml
        qml/Controls/ListRow.qml
        qml/Controls/NewDot.qml
        qml/Controls/NoticePill.qml
        qml/Controls/PersonAvatar.qml
        qml/Controls/PlainLabel.qml
        qml/Controls/ProgressRing.qml
        qml/Controls/Raised.qml
        qml/Controls/RollingText.qml
        qml/Controls/RoomSigil.qml
        qml/Controls/SegmentedPill.qml
        qml/Controls/SessionPlace.qml
        qml/Controls/ShimmerText.qml
        qml/Controls/SlotMark.qml
        qml/Controls/Squircle.qml
        qml/Controls/StartMark.qml
        qml/Controls/StatusSign.qml
        qml/Controls/Tag.qml
        qml/Controls/Well.qml
        qml/Controls/Wordmark.qml
        qml/Shell/AboutDialog.qml
        qml/Shell/CommandPalette.qml
        qml/Shell/QuitDialog.qml
        qml/Shell/Sidebar.qml
        qml/Shell/SidebarMenu.qml
        qml/Shell/SignOutDialog.qml
        qml/Shell/StartupOverlay.qml
        qml/Shell/WindowControls.qml
        qml/Theme/Identity.qml
        qml/Theme/KodosiTheme.qml
        qml/Theme/Mentions.qml
        qml/Workbench/AgentLauncher.qml
        qml/Workbench/ClipboardReadDialog.qml
        qml/Workbench/CloseTerminalDialog.qml
        qml/Workbench/EffortMeter.qml
        qml/Workbench/HistoryView.qml
        qml/Workbench/LauncherDropdown.qml
        qml/Workbench/LauncherPrompt.qml
        qml/Workbench/LauncherSheet.qml
        qml/Workbench/PickerKey.qml
        qml/Workbench/PickerList.qml
        qml/Workbench/SessionDetails.qml
        qml/Workbench/ShareSheet.qml
        qml/Workbench/StageOverview.qml
        qml/Workbench/TerminalDivider.qml
        qml/Workbench/TerminalStage.qml
        qml/Workbench/TerminalTile.qml
    RESOURCES
        qml/assets/kodosi-wordmark.png
        qml/assets/providers/claude-mark.svg
        qml/assets/providers/copilot-dark.svg
        qml/assets/providers/codex-dark.svg
        qml/assets/providers/cursor-dark.svg
        qml/assets/providers/gemini-dark.svg
        qml/assets/providers/opencode-dark.svg
)

target_link_libraries(kodosi-ui PUBLIC Qt6::Quick Qt6::QuickControls2 kodosi-qml-modelsplugin)
target_include_directories(kodosi-ui PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/controls")
set_source_files_properties(controls/LightTrack.cpp PROPERTIES COMPILE_OPTIONS "${KODOSI_COMPILE_OPTIONS}")

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
target_compile_definitions(kodosi-qt PRIVATE KODOSI_VERSION="${PROJECT_VERSION}")
foreach(target kodosi-runtime kodosi-application kodosi-desktop-integration kodosi-terminal-kernel kodosi-qt)
    target_compile_options(${target} PRIVATE ${KODOSI_COMPILE_OPTIONS})
endforeach()

add_custom_target(kodosi-qt-host-program ALL
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${KODOSI_RUNTIME_CLI}"
        "$<TARGET_FILE_DIR:kodosi-qt>/${KODOSI_RUNTIME_CLI_NAME}"
    COMMENT "Placing the kodosi program next to the app"
    VERBATIM
)
add_dependencies(kodosi-qt-host-program kodosi-qt kodosi-rust-cli-build)

kodosi_install(kodosi-qt)

