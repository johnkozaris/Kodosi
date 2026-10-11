# Notices and provenance that every package ships under one documentation
# folder. Release builds add the source identity, the Rust dependency
# licenses and the Qt licenses with Qt's own SBOMs.

set(KODOSI_GHOSTTY_LICENSE_TEXTS
    highway-Apache-2.0.txt
    highway-BSD-3-Clause.txt
    simdutf-MIT.txt
    simdutf-PyTorch-BSD-3-Clause.txt
    simdutf-Fuchsia-BSD-3-Clause.txt
    uucode.txt
    uucode-Bjoern-Hoehrmann.txt
    unicode-data.txt
    wuffs.txt
    zig-compiler-runtime.txt
    compiler-rt-LLVM-derived.txt
    zig-runtime-Apache-2.0-LLVM-exception.txt
    compiler-rt-musl-derived.txt
    zig-runtime-MIT-derived.txt
)

function(kodosi_install_evidence doc_dir)
    install(
        FILES "${PROJECT_SOURCE_DIR}/dependencies.lock.json"
        DESTINATION "${doc_dir}/provenance"
    )
    install(
        FILES
            "${KODOSI_GHOSTTY_ROOT}/LICENSE"
            "${KODOSI_GHOSTTY_ROOT}/LICENSE-GHOSTTY"
            "${KODOSI_GHOSTTY_ROOT}/${KODOSI_GHOSTTY_REF}"
            "${KODOSI_GHOSTTY_ROOT}/THIRD_PARTY_NOTICES.md"
        DESTINATION "${doc_dir}/ghostty"
    )
    list(TRANSFORM KODOSI_GHOSTTY_LICENSE_TEXTS
        PREPEND "${KODOSI_GHOSTTY_ROOT}/ThirdPartyNotices/licenses/"
        OUTPUT_VARIABLE ghostty_license_texts
    )
    install(
        FILES ${ghostty_license_texts}
        DESTINATION "${doc_dir}/ghostty/ThirdPartyNotices/licenses"
    )
    if(NOT CMAKE_BUILD_TYPE STREQUAL "Release")
        return()
    endif()

    set(rust_license_dir "${PROJECT_BINARY_DIR}/generated/rust-dependency-licenses")
    add_custom_target(kodosi-rust-license-inventory ALL
        COMMAND ${KODOSI_VERIFY_GHOSTTY_COMMAND}
        COMMAND
            "${Python3_EXECUTABLE}"
            "${PROJECT_SOURCE_DIR}/scripts/release/generate-rust-license-inventory.py"
            --manifest "${KODOSI_RUNTIME_MANIFEST}"
            --target "${KODOSI_RUST_TARGET}"
            --output-dir "${rust_license_dir}"
            --overrides
            "${PROJECT_SOURCE_DIR}/packaging/licenses/rust-license-overrides.json"
        BYPRODUCTS
            "${rust_license_dir}/rust-dependency-licenses.json"
        COMMENT "Generating the locked ${KODOSI_RUST_TARGET} Rust dependency license inventory"
        VERBATIM
    )
    add_dependencies(kodosi-rust-license-inventory kodosi-rust-cli-build)

    install(CODE "
        execute_process(
            COMMAND
                \"${Python3_EXECUTABLE}\"
                \"${PROJECT_SOURCE_DIR}/scripts/release/verify-release-sources.py\"
                --source-root
                \"${PROJECT_SOURCE_DIR}\"
            RESULT_VARIABLE KODOSI_INSTALL_SOURCE_RESULT
            ERROR_VARIABLE KODOSI_INSTALL_SOURCE_ERROR
        )
        if(NOT KODOSI_INSTALL_SOURCE_RESULT EQUAL 0)
            message(FATAL_ERROR
                \"Source verification immediately before install failed: \${KODOSI_INSTALL_SOURCE_ERROR}\"
            )
        endif()
    ")
    install(
        FILES "${KODOSI_SOURCE_IDENTITY}"
        DESTINATION "${doc_dir}/provenance"
    )
    install(
        DIRECTORY "${rust_license_dir}/"
        DESTINATION "${doc_dir}/rust"
    )
    install(
        DIRECTORY "${PROJECT_SOURCE_DIR}/packaging/licenses/qt/${KODOSI_QT_VERSION}/"
        DESTINATION "${doc_dir}/qt"
    )
    set(qt_sboms "")
    foreach(module IN LISTS KODOSI_QT_SBOM_MODULES)
        foreach(format spdx spdx.json cdx.json)
            list(APPEND qt_sboms "${QT6_INSTALL_PREFIX}/sbom/${module}-${KODOSI_QT_VERSION}.${format}")
        endforeach()
    endforeach()
    install(FILES ${qt_sboms} DESTINATION "${doc_dir}/qt/sbom")
endfunction()
