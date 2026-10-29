# Ubuntu lets only programs with an AppArmor profile make user namespaces, which the sandbox
# of canvas pages needs.
if(CPACK_GENERATOR STREQUAL "DEB")
    file(
        COPY "${CPACK_KODOSI_LINUX_PACKAGING}/apparmor/kodosi"
        DESTINATION "${CPACK_TEMPORARY_INSTALL_DIRECTORY}/etc/apparmor.d"
        FILE_PERMISSIONS OWNER_READ OWNER_WRITE GROUP_READ WORLD_READ
    )
endif()
