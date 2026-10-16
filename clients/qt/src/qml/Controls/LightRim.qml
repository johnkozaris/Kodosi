import Kodosi 1.0
import QtQuick

LightTrack {
    light: tile ? KodosiTheme.terminalAccentStrong : KodosiTheme.accent
    still: KodosiTheme.reduceMotion || !KodosiTheme.shown

    Accessible.ignored: true
}
