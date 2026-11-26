Kodosi redistributes the libavcodec, libavutil and libswresample libraries of
FFmpeg 9.0.1 as Qt 6.12.0 Multimedia ships them. Qt builds them without the
optional GPL parts of FFmpeg, so the GNU Lesser General Public License version
2.1 or later applies, as LICENSE.md explains. Qt's Multimedia SBOM, installed
with the Qt SBOMs, lists the build.

The libraries remain dynamically linked and replaceable inside Kodosi's
private runtime directory. The source archive's URL and digest are recorded
in Kodosi's native-license-evidence.json.
