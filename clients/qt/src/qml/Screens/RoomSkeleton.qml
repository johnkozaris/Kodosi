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
                Layout.fillWidth: true
                spacing: 12
                Rectangle { Layout.alignment: Qt.AlignTop; implicitWidth: 28; implicitHeight: 28; radius: 14; color: KodosiTheme.alpha(KodosiTheme.ink, 0.1) }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 9
                    Rectangle { implicitWidth: 90; implicitHeight: 8; radius: 4; color: KodosiTheme.alpha(KodosiTheme.ink, 0.1) }
                    Rectangle { Layout.fillWidth: true; implicitHeight: 9; radius: 4; color: KodosiTheme.alpha(KodosiTheme.ink, 0.1) }
                    Rectangle { implicitWidth: 150; implicitHeight: 9; radius: 4; color: KodosiTheme.alpha(KodosiTheme.ink, 0.1) }
                }
            }
        }
        Item { Layout.fillHeight: true }
    }
    SequentialAnimation {
        running: root.visible && !KodosiTheme.reduceMotion && KodosiTheme.shown
        loops: Animation.Infinite
        OpacityAnimator { target: root; from: 0.4; to: 0.9; duration: 850; easing.type: Easing.InOutSine }
        OpacityAnimator { target: root; from: 0.9; to: 0.4; duration: 850; easing.type: Easing.InOutSine }
    }
}
