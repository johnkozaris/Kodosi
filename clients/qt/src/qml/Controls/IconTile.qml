import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string iconName: ""
    property color tint: KodosiTheme.accent
    property real size: 28

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Squircle {
        anchors.fill: parent
        lightColor: KodosiTheme.mix(root.tint, "#ffffff", 0.22)
        baseColor: root.tint
        darkColor: KodosiTheme.mix(root.tint, "#000000", 0.2)
        rim: 0.75
        shadow: KodosiTheme.isDark ? 0.4 : 0.18
        shadowRadius: root.size * 0.08
        shadowX: 1
        shadowY: 2
    }
    KIcon {
        anchors.centerIn: parent
        width: root.size * 0.56
        height: width
        name: root.iconName
        color: "#ffffff"
        strokeWidth: 2
    }
}
