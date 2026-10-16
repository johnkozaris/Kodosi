import Kodosi 1.0
import QtQuick

CellTrack {
    strong: KodosiTheme.accentStrong
    accent: KodosiTheme.accent
    soft: KodosiTheme.accentSoft
    still: KodosiTheme.reduceMotion
    paused: !KodosiTheme.shown

    Accessible.ignored: true
}
