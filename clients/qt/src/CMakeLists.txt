set_source_files_properties(
    qml/Theme/KodosiTheme.qml
    qml/Theme/Identity.qml
    qml/Theme/Mentions.qml
    qml/Dosi/Fold.qml
    qml/Controls/Tip.qml
    qml/Controls/Prose.qml
    qml/Workbench/DocumentViews.qml
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE
)
set_source_files_properties(
    qml/Settings/GeneralSettings.qml
    qml/Settings/DeviceSettings.qml
    qml/Settings/TerminalSettings.qml
    qml/Settings/KeyboardSettings.qml
    qml/Settings/AgentSettings.qml
    qml/Settings/AgentDefaults.qml
    qml/Settings/DosiSettings.qml
    qml/Settings/BrowserSettings.qml
    qml/Settings/DocumentSettings.qml
    PROPERTIES QT_QML_INTERNAL_TYPE TRUE
)

qt_add_library(kodosi-runtime STATIC
    runtime/RuntimeBridge.cpp
    runtime/RuntimeBridge.hpp
    runtime/RuntimeEventFaults.hpp
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
    app/AppLanguage.cpp
    app/AppLanguage.hpp
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
    presentation/ProjectsModel.cpp
    presentation/ProjectsModel.hpp
    presentation/PeopleModel.cpp
    presentation/PeopleModel.hpp
    presentation/ProviderFilesModel.cpp
    presentation/ProviderFilesModel.hpp
    presentation/SessionActionsModel.cpp
    presentation/SessionActionsModel.hpp
    presentation/TerminalTypeAhead.cpp
    presentation/TerminalTypeAhead.hpp
    presentation/Workspace.cpp
    presentation/Workspace.hpp
    presentation/AppearanceModel.cpp
    presentation/AppearanceModel.hpp
    presentation/CanvasDocuments.cpp
    presentation/CanvasDocuments.hpp
    presentation/ChatProse.cpp
    presentation/ChatProse.hpp
    presentation/ChatWords.cpp
    presentation/ChatWords.hpp
    presentation/TerminalChat.cpp
    presentation/TerminalChat.hpp
    presentation/CanvasModel.cpp
    presentation/Link.cpp
    presentation/Link.hpp
    presentation/CanvasModel.hpp
    presentation/DesktopSettings.cpp
    presentation/DesktopSettings.hpp
    presentation/DesktopStateModel.cpp
    presentation/DesktopStateModel.hpp
    presentation/SessionCatalogModel.cpp
    presentation/SessionCatalogModel.hpp
    presentation/ShortcutsModel.cpp
    presentation/ShortcutsModel.hpp
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
        Qt6::GuiPrivate
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
    platform/Presence.cpp
    platform/Presence.hpp
    platform/System.hpp
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

if(LINUX)
    qt_generate_wayland_protocol_client_sources(kodosi-desktop-integration
        FILES "${CMAKE_CURRENT_SOURCE_DIR}/platform/linux/ext-idle-notify-v1.xml"
    )
endif()
if(WIN32)
    target_link_libraries(kodosi-desktop-integration PRIVATE Qt6::GuiPrivate)
endif()

qt_add_resources(kodosi-desktop-integration "tray"
    PREFIX "/kodosi"
    BASE platform
    FILES platform/kodosi-tray.svg
)

qt_add_library(kodosi-terminal-kernel STATIC
    accessibility/AccessibilityScope.cpp
    accessibility/AccessibilityScope.hpp
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

qt_add_library(kodosi-canvas-web STATIC
    canvas/BrowserVideo.cpp
    canvas/BrowserVideo.hpp
    canvas/ScreenVideo.cpp
    canvas/ScreenVideo.hpp
    canvas/StreamVideo.cpp
    canvas/StreamVideo.hpp
    canvas/VideoDecoder.cpp
    canvas/VideoDecoder.hpp
    canvas/VideoNode.cpp
    canvas/VideoNode.hpp
    canvas/CanvasImages.cpp
    canvas/CanvasImages.hpp
    canvas/CanvasWeb.cpp
    canvas/CanvasWeb.hpp
)

target_include_directories(kodosi-canvas-web
    PUBLIC
        "${CMAKE_CURRENT_SOURCE_DIR}"
)

target_link_libraries(kodosi-canvas-web
    PUBLIC
        Qt6::Quick
        Qt6::WebEngineQuick
        kodosi-application
    PRIVATE
        Qt6::WebEngineCore
        Kodosi::RustRuntime
)

qt_add_shaders(kodosi-canvas-web "kodosi-video-shaders"
    BATCHABLE
    PREFIX "/kodosi/shaders"
    BASE canvas/shaders
    FILES
        canvas/shaders/video.vert
        canvas/shaders/video.frag
)
target_link_libraries(kodosi-canvas-web PRIVATE Qt6::GuiPrivate FFmpeg::avcodec FFmpeg::avutil)
if(WIN32)
    target_sources(kodosi-canvas-web PRIVATE canvas/VideoGpuWindows.cpp)
    target_link_libraries(kodosi-canvas-web PRIVATE d3d11 dxgi dxguid)
else()
    find_package(OpenGL REQUIRED COMPONENTS EGL)
    target_sources(kodosi-canvas-web PRIVATE canvas/VideoGpuLinux.cpp)
    target_link_libraries(kodosi-canvas-web PRIVATE OpenGL::EGL FFmpeg::va)
endif()

qt_add_library(kodosi-qml-models STATIC)
qt_add_qml_module(kodosi-qml-models
    URI Kodosi.Models
    VERSION 1.0
    OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/Kodosi/Models"
    DEPENDENCIES
        QtQuick
        QtWebEngine
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
        kodosi-canvas-web
        kodosi-terminal-kernel
)

target_include_directories(kodosi-terminal-kernel PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")

target_link_libraries(kodosi-terminal-kernel
    PUBLIC
        Qt6::Core
        Qt6::Gui
        Qt6::Quick
        kodosi-application
    PRIVATE
        Qt6::QuickPrivate
        Kodosi::RustRuntime
        kodosi-ghostty-vt
)

qt_add_executable(kodosi-qt
    app/main.cpp
)

qt_add_library(kodosi-ui STATIC)
qt_add_qml_module(kodosi-ui
    URI Kodosi
    OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/Kodosi"
    VERSION 1.0
    DEPENDENCIES
        Kodosi.Models
        QtWebChannel
    SOURCES
        controls/KeyRecorder.cpp
        controls/KeyRecorder.hpp
        controls/LightTrack.cpp
        controls/LightTrack.hpp
        controls/SignClock.cpp
        controls/SignClock.hpp
    QML_FILES
        qml/Main.qml
        qml/Screens/PeopleView.qml
        qml/Screens/BlockPersonDialog.qml
        qml/Screens/ReportPersonDialog.qml
        qml/Screens/ReportReceived.qml
        qml/Screens/FriendTrustDialog.qml
        qml/Screens/IssuePicker.qml
        qml/Screens/RepositoryPicker.qml
        qml/Screens/ProjectsView.qml
        qml/Screens/ProjectContent.qml
        qml/Screens/ProjectSkeleton.qml
        qml/Screens/ProjectConversation.qml
        qml/Screens/ProjectTasks.qml
        qml/Screens/ProjectRepositories.qml
        qml/Settings/SettingsView.qml
        qml/Settings/GeneralSettings.qml
        qml/Settings/DeviceSettings.qml
        qml/Settings/TerminalSettings.qml
        qml/Settings/KeyboardSettings.qml
        qml/Settings/AgentSettings.qml
        qml/Settings/AgentDefaults.qml
        qml/Settings/BrowserSettings.qml
        qml/Settings/DocumentSettings.qml
        qml/Settings/DosiSettings.qml
        qml/Chat/ChatAskCard.qml
        qml/Chat/ChatDock.qml
        qml/Chat/ChatFacts.qml
        qml/Chat/ChatStep.qml
        qml/Chat/ChatThread.qml
        qml/Chat/ChatWork.qml
        qml/Chat/TerminalChatView.qml
        qml/Controls/AccountMark.qml
        qml/Controls/AgentProse.qml
        qml/Controls/Arrive.qml
        qml/Controls/ChangeLines.qml
        qml/Controls/CodeWell.qml
        qml/Controls/ComposerBox.qml
        qml/Controls/DayDivider.qml
        qml/Controls/LatestKey.qml
        qml/Controls/PersonBubble.qml
        qml/Controls/Prose.qml
        qml/Controls/SendKey.qml
        qml/Controls/StopKey.qml
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
        qml/Controls/HelperMarks.qml
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
        qml/Controls/KPicker.qml
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
        qml/Controls/ReconnectingNotice.qml
        qml/Controls/ScanCode.qml
        qml/Controls/RollingText.qml
        qml/Controls/ProjectSigil.qml
        qml/Controls/SegmentedPill.qml
        qml/Controls/SessionPlace.qml
        qml/Controls/ShareBadge.qml
        qml/Controls/ShimmerText.qml
        qml/Controls/SlotMark.qml
        qml/Controls/Squircle.qml
        qml/Controls/StatusSign.qml
        qml/Controls/Tag.qml
        qml/Controls/TerminalDragSource.qml
        qml/Controls/TerminalDropArea.qml
        qml/Controls/UsageDial.qml
        qml/Controls/UsageRing.qml
        qml/Controls/Well.qml
        qml/Controls/Wordmark.qml
        qml/Shell/AboutDialog.qml
        qml/Shell/CommandPalette.qml
        qml/Shell/QuitDialog.qml
        qml/Shell/Sidebar.qml
        qml/Shell/SidebarMenu.qml
        qml/Shell/SignOutDialog.qml
        qml/Shell/StartupOverlay.qml
        qml/Shell/UsageMenu.qml
        qml/Shell/WindowControls.qml
        qml/Theme/Identity.qml
        qml/Theme/KodosiTheme.qml
        qml/Theme/Look.qml
        qml/Theme/Mentions.qml
        qml/Workbench/AgentChoiceMenu.qml
        qml/Workbench/AgentLauncher.qml
        qml/Workbench/CanvasBody.qml
        qml/Workbench/CanvasBrowser.qml
        qml/Workbench/CanvasDock.qml
        qml/Workbench/CanvasDocument.qml
        qml/Workbench/CanvasFile.qml
        qml/Workbench/CanvasPage.qml
        qml/Workbench/CanvasPanel.qml
        qml/Workbench/CanvasScreen.qml
        qml/Workbench/DocumentViews.qml
        qml/Workbench/CanvasStage.qml
        qml/Workbench/ClipboardReadDialog.qml
        qml/Workbench/CloseTerminalDialog.qml
        qml/Workbench/EffortMeter.qml
        qml/Workbench/HistoryView.qml
        qml/Workbench/LauncherDropdown.qml
        qml/Workbench/LauncherPrompt.qml
        qml/Workbench/LauncherSheet.qml
        qml/Workbench/LinkDialog.qml
        qml/Workbench/ModeRow.qml
        qml/Workbench/OfflineTerminalDialog.qml
        qml/Workbench/PickerKey.qml
        qml/Workbench/PickerList.qml
        qml/Workbench/RemoveKey.qml
        qml/Workbench/ProjectShareDialog.qml
        qml/Workbench/SessionDetails.qml
        qml/Workbench/ShareSheet.qml
        qml/Workbench/StageOverview.qml
        qml/Workbench/TerminalActions.qml
        qml/Workbench/TerminalDivider.qml
        qml/Workbench/TerminalStage.qml
        qml/Workbench/TerminalTile.qml
    RESOURCES
        qml/assets/kodosi-wordmark.png
        qml/assets/providers/antigravity-dark.svg
        qml/assets/providers/claude-mark.svg
        qml/assets/providers/copilot-dark.svg
        qml/assets/providers/codex-dark.svg
        qml/assets/providers/cursor-dark.svg
        qml/assets/providers/opencode-dark.svg
)

target_link_libraries(kodosi-ui PUBLIC Qt6::Quick Qt6::QuickControls2 Qt6::WebEngineQuick kodosi-qml-modelsplugin
    PRIVATE Qt6::GuiPrivate)
target_include_directories(kodosi-ui PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/controls")
set_source_files_properties(controls/KeyRecorder.cpp controls/LightTrack.cpp controls/SignClock.cpp
    PROPERTIES COMPILE_OPTIONS "${KODOSI_COMPILE_OPTIONS}")

target_link_libraries(kodosi-qt
    PRIVATE
        Kodosi::RustRuntime
        Qt6::Core
        Qt6::Gui
        Qt6::Qml
        Qt6::Quick
        Qt6::WebChannelQuick
        Qt6::Widgets
        kodosi-desktop-integration
        kodosi-application
        kodosi-terminal-kernel
        kodosi-canvas-web
        kodosi-qml-modelsplugin
        kodosi-uiplugin
)

set(KODOSI_LANGUAGES de el es fr it ja ko zh_CN)
list(TRANSFORM KODOSI_LANGUAGES REPLACE "^(.+)$" "${CMAKE_CURRENT_SOURCE_DIR}/../i18n/kodosi_\\1.ts"
    OUTPUT_VARIABLE KODOSI_TS_FILES)
set(KODOSI_PLURALS_TS_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../i18n/kodosi_en.ts")
qt_add_lupdate(
    TS_FILES ${KODOSI_TS_FILES}
    PLURALS_TS_FILE "${KODOSI_PLURALS_TS_FILE}"
    SOURCE_TARGETS kodosi-qt kodosi-ui kodosi-application kodosi-qml-models kodosi-desktop-integration
        kodosi-canvas-web kodosi-terminal-kernel kodosi-runtime
    LUPDATE_TARGET kodosi-translations-update
    OPTIONS -no-obsolete -locations none
    NO_GLOBAL_TARGET
)
set(KODOSI_TS_CHECK_DIR "${CMAKE_CURRENT_BINARY_DIR}/translations-check")
list(TRANSFORM KODOSI_LANGUAGES REPLACE "^(.+)$" "${KODOSI_TS_CHECK_DIR}/kodosi_\\1.ts"
    OUTPUT_VARIABLE KODOSI_TS_CHECK_FILES)
add_custom_target(kodosi-translations-check-copy
    COMMAND "${CMAKE_COMMAND}" -E rm -rf "${KODOSI_TS_CHECK_DIR}"
    COMMAND "${CMAKE_COMMAND}" -E make_directory "${KODOSI_TS_CHECK_DIR}"
    COMMAND "${CMAKE_COMMAND}" -E copy ${KODOSI_TS_FILES} "${KODOSI_PLURALS_TS_FILE}" "${KODOSI_TS_CHECK_DIR}"
)
qt_add_lupdate(
    TS_FILES ${KODOSI_TS_CHECK_FILES}
    PLURALS_TS_FILE "${KODOSI_TS_CHECK_DIR}/kodosi_en.ts"
    SOURCE_TARGETS kodosi-qt kodosi-ui kodosi-application kodosi-qml-models kodosi-desktop-integration
        kodosi-canvas-web kodosi-terminal-kernel kodosi-runtime
    LUPDATE_TARGET kodosi-translations-check
    OPTIONS -no-obsolete -locations none
    NO_GLOBAL_TARGET
)
add_dependencies(kodosi-translations-check kodosi-translations-check-copy)
qt_add_lrelease(
    TS_FILES ${KODOSI_TS_FILES} "${KODOSI_PLURALS_TS_FILE}"
    QM_FILES_OUTPUT_VARIABLE KODOSI_QM_FILES
    LRELEASE_TARGET kodosi-translations-release
    NO_GLOBAL_TARGET
)
qt_add_resources(kodosi-qt kodosi-translations
    PREFIX /i18n
    BASE "${CMAKE_CURRENT_BINARY_DIR}"
    FILES ${KODOSI_QM_FILES}
)
# Qt words its own texts in only some of these languages.
set(KODOSI_QT_LANGUAGES "")
foreach(language IN LISTS KODOSI_LANGUAGES)
    if(EXISTS "${QT6_INSTALL_PREFIX}/${QT6_INSTALL_TRANSLATIONS}/qtbase_${language}.qm")
        list(APPEND KODOSI_QT_LANGUAGES "${language}")
    endif()
endforeach()
string(REPLACE "_" "-" KODOSI_CHROMIUM_LANGUAGES "en-US;${KODOSI_LANGUAGES}")

target_compile_definitions(kodosi-runtime
    PUBLIC
        QT_ENABLE_STRICT_MODE_UP_TO=0x060C00
        QT_NO_CAST_FROM_ASCII
)
target_compile_definitions(kodosi-qt PRIVATE KODOSI_VERSION="${PROJECT_VERSION}")
foreach(target kodosi-runtime kodosi-application kodosi-desktop-integration kodosi-terminal-kernel kodosi-canvas-web kodosi-qt)
    target_compile_options(${target} PRIVATE "$<$<COMPILE_LANGUAGE:CXX>:${KODOSI_COMPILE_OPTIONS}>")
endforeach()

add_custom_target(kodosi-qt-host-program ALL
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${KODOSI_RUNTIME_CLI}"
        "$<TARGET_FILE_DIR:kodosi-qt>/${KODOSI_RUNTIME_CLI_NAME}"
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${KODOSI_AGENT_BROWSER}"
        "$<TARGET_FILE_DIR:kodosi-qt>/${KODOSI_AGENT_BROWSER_NAME}"
    COMMAND "${CMAKE_COMMAND}" -E copy_directory "${KODOSI_DOCUMENT_VIEWS}"
        "$<TARGET_FILE_DIR:kodosi-qt>/DocumentViews"
    COMMENT "Placing the kodosi program, agent-browser and the document views next to the app"
    VERBATIM
)
add_dependencies(kodosi-qt-host-program kodosi-qt kodosi-runtime-library-build kodosi-agent-browser-build
    kodosi-document-views-build)

kodosi_install(kodosi-qt)

