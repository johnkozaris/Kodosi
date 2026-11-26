# Qt Multimedia ships FFmpeg, built by Qt with the decoders of every GPU, but without its headers or Windows
# import libraries. The headers come from the same FFmpeg release, and the import libraries from the
# functions that Kodosi calls.
string(JSON KODOSI_FFMPEG_VERSION GET "${KODOSI_DEPENDENCIES}" frameworks ffmpeg version)
string(JSON ffmpeg_source GET "${KODOSI_DEPENDENCIES}" frameworks ffmpeg source)
string(JSON ffmpeg_sha256 GET "${KODOSI_DEPENDENCIES}" frameworks ffmpeg sha256)
set(ffmpeg_root "${CMAKE_BINARY_DIR}/ffmpeg-${KODOSI_FFMPEG_VERSION}")
set(ffmpeg_include "${ffmpeg_root}/include")

if(NOT EXISTS "${ffmpeg_include}/libavutil/avconfig.h")
    set(archive "${ffmpeg_root}/ffmpeg-${KODOSI_FFMPEG_VERSION}.tar.xz")
    file(DOWNLOAD "${ffmpeg_source}" "${archive}" EXPECTED_HASH SHA256=${ffmpeg_sha256} TLS_VERIFY ON STATUS status)
    list(GET status 0 failure)
    if(failure)
        message(FATAL_ERROR "Could not download the FFmpeg ${KODOSI_FFMPEG_VERSION} headers: ${status}")
    endif()
    file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${ffmpeg_root}/source"
        PATTERNS "*/libavcodec/*.h" "*/libavutil/*.h" "*/libswresample/version*.h")
    file(COPY
        "${ffmpeg_root}/source/ffmpeg-${KODOSI_FFMPEG_VERSION}/libavcodec"
        "${ffmpeg_root}/source/ffmpeg-${KODOSI_FFMPEG_VERSION}/libavutil"
        "${ffmpeg_root}/source/ffmpeg-${KODOSI_FFMPEG_VERSION}/libswresample"
        DESTINATION "${ffmpeg_include}")
    file(REMOVE_RECURSE "${ffmpeg_root}/source")
    file(REMOVE "${archive}")
    # FFmpeg's configure writes this header. Every processor that Kodosi runs on is little-endian and
    # reads unaligned memory fast.
    file(WRITE "${ffmpeg_include}/libavutil/avconfig.h"
        "#ifndef AVUTIL_AVCONFIG_H\n#define AVUTIL_AVCONFIG_H\n"
        "#define AV_HAVE_BIGENDIAN 0\n#define AV_HAVE_FAST_UNALIGNED 1\n#endif\n")
endif()

function(kodosi_ffmpeg_major library header)
    file(STRINGS "${ffmpeg_include}/lib${library}/${header}" line
        REGEX "#define LIB[A-Z]+_VERSION_MAJOR")
    string(REGEX MATCH "[0-9]+$" major "${line}")
    set(ffmpeg_${library}_major "${major}" PARENT_SCOPE)
endfunction()
kodosi_ffmpeg_major(avcodec version_major.h)
kodosi_ffmpeg_major(avutil version.h)
kodosi_ffmpeg_major(swresample version_major.h)

foreach(library avcodec avutil)
    add_library(FFmpeg::${library} SHARED IMPORTED)
    set_target_properties(FFmpeg::${library} PROPERTIES INTERFACE_INCLUDE_DIRECTORIES "${ffmpeg_include}")
    if(WIN32)
        set(dll "${library}-${ffmpeg_${library}_major}.dll")
        set(import_library "${ffmpeg_root}/${library}.lib")
        # llvm-lib takes the DLL's name only from the definitions' LIBRARY statement; without one,
        # every import names an empty DLL and the program cannot start.
        set(definitions "${ffmpeg_root}/${library}.def")
        file(READ "${PROJECT_SOURCE_DIR}/cmake/ffmpeg/${library}.def" exports)
        file(WRITE "${definitions}" "LIBRARY ${dll}\n${exports}")
        execute_process(
            COMMAND "${CMAKE_AR}" "/def:${definitions}" "/machine:${KODOSI_WINDOWS_ARCH}" "/out:${import_library}"
            COMMAND_ERROR_IS_FATAL ANY
            OUTPUT_QUIET
        )
        set_target_properties(FFmpeg::${library} PROPERTIES
            IMPORTED_LOCATION "${QT6_INSTALL_PREFIX}/bin/${dll}"
            IMPORTED_IMPLIB "${import_library}"
        )
    else()
        set(soname "lib${library}.so.${ffmpeg_${library}_major}")
        set_target_properties(FFmpeg::${library} PROPERTIES
            IMPORTED_LOCATION "${QT6_INSTALL_PREFIX}/lib/${soname}"
            IMPORTED_SONAME "${soname}"
            # The libraries need Qt's stubs for libva and OpenSSL, which lie beside them. The OpenSSL
            # stub defines no functions: libavutil binds its random bytes, which decoding never asks for,
            # only when it calls them.
            INTERFACE_LINK_OPTIONS "LINKER:-rpath-link,${QT6_INSTALL_PREFIX}/lib;LINKER:--allow-shlib-undefined"
        )
    endif()
endforeach()
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    "${PROJECT_SOURCE_DIR}/cmake/ffmpeg/avcodec.def" "${PROJECT_SOURCE_DIR}/cmake/ffmpeg/avutil.def")

if(WIN32)
    set(KODOSI_FFMPEG_RUNTIME
        "${QT6_INSTALL_PREFIX}/bin/avcodec-${ffmpeg_avcodec_major}.dll"
        "${QT6_INSTALL_PREFIX}/bin/avutil-${ffmpeg_avutil_major}.dll"
        "${QT6_INSTALL_PREFIX}/bin/swresample-${ffmpeg_swresample_major}.dll"
    )
else()
    # Qt's FFmpeg reaches libva through these stubs, which load libva only when the system has it.
    add_library(FFmpeg::va SHARED IMPORTED)
    set_target_properties(FFmpeg::va PROPERTIES
        IMPORTED_LOCATION "${QT6_INSTALL_PREFIX}/lib/libQt6FFmpegStub-va.so.2"
        IMPORTED_SONAME "libQt6FFmpegStub-va.so.2"
    )
endif()
