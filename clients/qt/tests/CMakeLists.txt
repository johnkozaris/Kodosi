function(kodosi_test name source)
    qt_add_executable(${name} ${source})
    target_link_libraries(${name} PRIVATE
        kodosi-application kodosi-desktop-integration kodosi-terminal-kernel
        kodosi-ghostty-vt Qt6::Test Qt6::Widgets Qt6::QuickControls2)
    target_compile_definitions(${name} PRIVATE KODOSI_SOURCE_DIR="${PROJECT_SOURCE_DIR}")
    add_test(NAME ${name} COMMAND ${KODOSI_TEST_LAUNCHER} "$<TARGET_FILE:${name}>")
    set_tests_properties(${name} PROPERTIES
        ENVIRONMENT "${KODOSI_TEST_ENVIRONMENT}"
        ENVIRONMENT_MODIFICATION "${KODOSI_TEST_ENVIRONMENT_MODIFICATION}"
        TIMEOUT 60)
endfunction()

qt_add_executable(kodosi-runtime-tests tst_runtime_bridge.cpp RuntimeApiFixture.cpp)
target_link_libraries(kodosi-runtime-tests PRIVATE kodosi-runtime Kodosi::RustRuntime Qt6::Test)
target_compile_definitions(kodosi-runtime-tests PRIVATE KODOSI_SOURCE_DIR="${PROJECT_SOURCE_DIR}")
add_test(NAME kodosi-runtime-tests COMMAND kodosi-runtime-tests)
set_tests_properties(kodosi-runtime-tests PROPERTIES
    ENVIRONMENT_MODIFICATION "${KODOSI_TEST_ENVIRONMENT_MODIFICATION}")

kodosi_test(kodosi-workspace-tests tst_workspace.cpp)
kodosi_test(kodosi-canvas-model-tests tst_canvas_model.cpp)
kodosi_test(kodosi-dosi-model-tests tst_dosi_model.cpp)
kodosi_test(kodosi-keyed-list-tests tst_keyed_list.cpp)
kodosi_test(kodosi-provider-models-tests tst_provider_models.cpp)
kodosi_test(kodosi-desktop-settings-tests tst_desktop_settings.cpp)
kodosi_test(kodosi-desktop-file-integration-tests tst_desktop_file_integration.cpp)
kodosi_test(kodosi-deep-links-tests tst_deep_links.cpp)
kodosi_test(kodosi-app-activation-tests tst_app_activation.cpp)
kodosi_test(kodosi-application-lifecycle-model-tests tst_application_lifecycle_model.cpp)
kodosi_test(kodosi-appearance-model-tests tst_appearance_model.cpp)
kodosi_test(kodosi-terminal-tiling-layout-model-tests tst_terminal_tiling_layout_model.cpp)
kodosi_test(kodosi-terminal-kernel-tests tst_terminal_kernel.cpp)
kodosi_test(kodosi-terminal-notifications-tests tst_terminal_notifications.cpp)
kodosi_test(kodosi-terminal-type-ahead-tests tst_terminal_type_ahead.cpp)
if(LINUX)
    kodosi_test(kodosi-freedesktop-notification-driver-tests tst_freedesktop_notification_driver.cpp)
endif()
if(WIN32)
    kodosi_test(kodosi-package-update-tests tst_package_update.cpp)
endif()
kodosi_test(kodosi-accessibility-scope-tests tst_accessibility_scope.cpp)

kodosi_test(kodosi-control-tests tst_controls.cpp)
target_link_libraries(kodosi-control-tests PRIVATE kodosi-uiplugin)
kodosi_test(kodosi-light-track-tests tst_light_track.cpp)
target_link_libraries(kodosi-light-track-tests PRIVATE kodosi-uiplugin)
