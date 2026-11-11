import Kodosi 1.0
import QtQuick

Rectangle {
    id: root

    property string project: ""
    property color ring: KodosiTheme.ground

    width: 12
    height: 12
    radius: 4
    color: ring
    Accessible.ignored: true

    Rectangle {
        anchors.centerIn: parent
        width: 8
        height: 8
        radius: 2.5
        color: root.project.length > 0 ? Identity.projectTint(root.project) : KodosiTheme.inkFaint
    }
}
