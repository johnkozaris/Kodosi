import Kodosi 1.0
import QtQuick

Item {
    id: root

    property var session: null
    property bool unseen: false
    property real size: 14
    property color backing: "transparent"
    readonly property string form: session && session.sign ? (session.sign === "hand" || session.sign === "question" || session.sign === "key" || unseen ? session.sign : "")
        : unseen ? "changed" : ""
    readonly property color tint: form === "done" ? KodosiTheme.ready : form === "failed" ? KodosiTheme.caution : KodosiTheme.accent
    readonly property string label: {
        switch (form) {
        case "changed": return qsTr("New activity")
        case "hand": return qsTr("Needs your approval")
        case "question": return qsTr("Needs your answer")
        case "key": return qsTr("Needs you to sign in")
        case "done": return qsTr("Done")
        case "failed": return qsTr("Failed")
        default: return ""
        }
    }
    property real drawn: 1
    property bool ready: false

    implicitWidth: size + 5
    implicitHeight: size + 5
    visible: form.length > 0
    Accessible.role: Accessible.StaticText
    Accessible.name: label
    Accessible.ignored: !visible

    Component.onCompleted: ready = true
    onFormChanged: {
        if (!ready || KodosiTheme.reduceMotion || form.length === 0)
            return
        arrive.restart()
        if (form === "hand")
            wave.restart()
        if (form === "done")
            draw.restart()
    }

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: root.backing
    }
    BreathingDot {
        anchors.centerIn: parent
        visible: root.form === "changed"
        size: Math.round(root.size * 0.6)
    }
    Item {
        id: glyph
        anchors.centerIn: parent
        width: root.size
        height: root.size
        visible: root.form.length > 0 && root.form !== "changed"
        transformOrigin: Item.Bottom

        Item {
            width: glyph.width * root.drawn
            height: glyph.height
            clip: true

            KIcon {
                width: glyph.width
                height: glyph.height
                name: root.form === "done" ? "check" : root.form === "failed" ? "warning" : root.form
                color: root.tint
                strokeWidth: 2.6
            }
        }
    }
    NumberAnimation { id: arrive; target: glyph; property: "scale"; from: 0.6; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
    NumberAnimation { id: draw; target: root; property: "drawn"; from: 0; to: 1; duration: 320; easing.type: Easing.OutCubic }
    SequentialAnimation {
        id: wave
        NumberAnimation { target: glyph; property: "rotation"; to: -18; duration: 130; easing.type: Easing.OutQuad }
        NumberAnimation { target: glyph; property: "rotation"; to: 14; duration: 170; easing.type: Easing.InOutQuad }
        NumberAnimation { target: glyph; property: "rotation"; to: -8; duration: 150; easing.type: Easing.InOutQuad }
        NumberAnimation { target: glyph; property: "rotation"; to: 0; duration: 150; easing.type: Easing.OutQuad }
    }
}
