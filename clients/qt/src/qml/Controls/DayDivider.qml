import Kodosi 1.0
import QtQuick

Item {
    id: root

    required property date date
    property Look look: KodosiTheme.look
    readonly property string label: {
        const today = new Date()
        const start = value => new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime()
        const days = Math.round((start(today) - start(date)) / 86400000)
        return days === 0 ? qsTr("Today") : days === 1 ? qsTr("Yesterday") : date.toLocaleDateString(Qt.locale(), qsTr("d MMMM"))
    }

    implicitHeight: KodosiTheme.scaled(26)
    Accessible.role: Accessible.StaticText
    Accessible.name: label

    Rectangle {
        anchors.centerIn: parent
        width: day.implicitWidth + 20
        height: KodosiTheme.scaled(20)
        radius: height / 2
        color: KodosiTheme.alpha(root.look.ink, 0.07)

        PlainLabel {
            id: day

            anchors.centerIn: parent
            text: root.label
            color: root.look.inkMuted
            font.pixelSize: KodosiTheme.fontCaption
            font.weight: Font.Medium
        }
    }
}
