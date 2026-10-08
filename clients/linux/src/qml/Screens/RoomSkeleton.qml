pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Kodosi 1.0

Item {
    id: root
    Accessible.role: Accessible.ProgressBar
    Accessible.name: qsTr("Loading")
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 26
        Repeater {
            model: 4
            delegate: RowLayout {
                required property int index
                Layout.fillWidth: true
                spacing: 12
                Rectangle { Layout.alignment: Qt.AlignTop; implicitWidth: 28; implicitHeight: 28; radius: 14; color: KodosiTheme.surfaceSelected }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 9
                    Rectangle { implicitWidth: 90; implicitHeight: 8; radius: 4; color: KodosiTheme.surfaceSelected }
                    Rectangle { Layout.fillWidth: true; implicitHeight: 9; radius: 4; color: KodosiTheme.surfaceSelected }
                    Rectangle { implicitWidth: 150; implicitHeight: 9; radius: 4; color: KodosiTheme.surfaceSelected }
                }
            }
        }
        Item { Layout.fillHeight: true }
    }
    SequentialAnimation on opacity {
        running: root.visible && !KodosiTheme.reduceMotion
        loops: Animation.Infinite
        NumberAnimation { from: 0.4; to: 0.8; duration: 850; easing.type: Easing.InOutSine }
        NumberAnimation { from: 0.8; to: 0.4; duration: 850; easing.type: Easing.InOutSine }
    }
}
