import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

// The canvas key in a terminal's header: a breathing dot and one hop for news, and raised
// while the canvas is open.
KIconButton {
    id: root

    required property string sessionId
    readonly property bool news: !active && Models.Canvas.revision >= 0 && Models.Canvas.hasNews(sessionId)

    glyph: "canvas"
    size: 26
    onTerminal: true
    Accessible.name: qsTr("Canvas")
    Accessible.description: news ? qsTr("Something new") : ""
    transform: Translate { id: lift }

    onNewsChanged: {
        if (news && !KodosiTheme.reduceMotion)
            hop.restart();
    }

    BreathingDot {
        x: root.width - width - 3
        y: 3
        size: 6
        color: KodosiTheme.glowOrange
        visible: root.news
    }
    SequentialAnimation {
        id: hop

        NumberAnimation { target: lift; property: "y"; to: -4; duration: 140; easing.type: Easing.OutQuad }
        NumberAnimation { target: lift; property: "y"; to: 0; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
    }
}
