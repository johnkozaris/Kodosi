# CPack runs this script before it makes each package. Only the Debian package gets
# the Kodosi apt repository and its key. file(COPY), because file(INSTALL) would put
# DESTDIR in front of the staging folder.
if(CPACK_GENERATOR STREQUAL "DEB")
    file(
        COPY "${CPACK_KODOSI_LINUX_PACKAGING}/kodosi.sources"
        DESTINATION "${CPACK_TEMPORARY_INSTALL_DIRECTORY}/etc/apt/sources.list.d"
        FILE_PERMISSIONS OWNER_READ OWNER_WRITE GROUP_READ WORLD_READ
    )
    file(
        COPY "${CPACK_KODOSI_LINUX_PACKAGING}/kodosi-archive-keyring.pgp"
        DESTINATION "${CPACK_TEMPORARY_INSTALL_DIRECTORY}/etc/apt/keyrings"
        FILE_PERMISSIONS OWNER_READ OWNER_WRITE GROUP_READ WORLD_READ
    )
endif()
