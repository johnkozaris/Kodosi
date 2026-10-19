import Kodosi 1.0
import QtQuick

Translate {
    id: root

    property int delay: 0
    property real rise: 8
    property real progress: 1
    property SequentialAnimation motion: SequentialAnimation {
        PauseAnimation { duration: root.delay }
        NumberAnimation { target: root; property: "progress"; to: 1; duration: 420; easing.type: Easing.OutCubic }
    }

    function play() {
        motion.stop()
        if (KodosiTheme.reduceMotion || !KodosiTheme.shown) {
            progress = 1
            return
        }
        progress = 0
        motion.start()
    }

    y: (1 - progress) * rise
}
