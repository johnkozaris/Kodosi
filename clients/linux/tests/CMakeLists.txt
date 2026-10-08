function(kodosi_test name source)
    qt_add_executable(${name} ${source})
    target_link_libraries(${name} PRIVATE
        kodosi-application kodosi-desktop-integration kodosi-terminal-kernel
        kodosi-ghostty-vt Qt6::Test Qt6::Widgets Qt6::QuickControls2)
    target_compile_definitions(${name} PRIVATE KODOSI_SOURCE_DIR="${PROJECT_SOURCE_DIR}")
    add_test(NAME ${name} COMMAND "${DBUS_RUN_SESSION_EXECUTABLE}" -- "$<TARGET_FILE:${name}>")
    set_tests_properties(${name} PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software"
        TIMEOUT 60)
endfunction()

qt_add_executable(kodosi-runtime-tests tst_runtime_bridge.cpp RuntimeApiFixture.cpp)
target_link_libraries(kodosi-runtime-tests PRIVATE kodosi-runtime Kodosi::RustRuntime Qt6::Test)
add_test(NAME kodosi-runtime-tests COMMAND kodosi-runtime-tests)

kodosi_test(kodosi-workspace-tests tst_workspace.cpp)
kodosi_test(kodosi-provider-models-tests tst_provider_models.cpp)
kodosi_test(kodosi-desktop-settings-tests tst_desktop_settings.cpp)
kodosi_test(kodosi-desktop-file-integration-tests tst_desktop_file_integration.cpp)
kodosi_test(kodosi-deep-links-tests tst_deep_links.cpp)
kodosi_test(kodosi-single-instance-tests tst_single_instance.cpp)
kodosi_test(kodosi-application-lifecycle-model-tests tst_application_lifecycle_model.cpp)
kodosi_test(kodosi-application-log-store-tests tst_application_log_store.cpp)
kodosi_test(kodosi-appearance-model-tests tst_appearance_model.cpp)
kodosi_test(kodosi-terminal-tiling-layout-model-tests tst_terminal_tiling_layout_model.cpp)
kodosi_test(kodosi-terminal-kernel-tests tst_terminal_kernel.cpp)
kodosi_test(kodosi-terminal-notifications-tests tst_terminal_notifications.cpp)
kodosi_test(kodosi-freedesktop-notification-driver-tests tst_freedesktop_notification_driver.cpp)
kodosi_test(kodosi-accessibility-scope-tests tst_accessibility_scope.cpp)

kodosi_test(kodosi-control-tests tst_controls.cpp)
target_link_libraries(kodosi-control-tests PRIVATE kodosi-uiplugin)
