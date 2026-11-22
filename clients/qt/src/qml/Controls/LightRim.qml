import Kodosi 1.0
import QtQuick

LightTrack {
    light: tile ? KodosiTheme.darkLook.accentStrong : KodosiTheme.accent
    still: KodosiTheme.reduceMotion || !KodosiTheme.shown

    Accessible.ignored: true
}
