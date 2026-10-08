import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root
    property string program: ""
    readonly property string provider: program.toLowerCase().indexOf("claude") >= 0 ? "claude"
        : program.toLowerCase().indexOf("copilot") >= 0 ? "copilot"
        : program.toLowerCase().indexOf("codex") >= 0 ? "codex"
        : program.toLowerCase().indexOf("cursor") >= 0 ? "cursor" : ""
    implicitWidth: 18
    implicitHeight: 18
    Image {
        anchors.fill: parent
        visible: root.provider.length > 0
        source: root.provider ? "qrc:/qt/qml/Kodosi/qml/assets/providers/" + root.provider + (Models.Appearance.dark ? "-dark.svg" : "-light.svg") : ""
        sourceSize.width: 36
        sourceSize.height: 36
    }
    KIcon { anchors.fill: parent; name: "terminal"; visible: !root.provider }
}
