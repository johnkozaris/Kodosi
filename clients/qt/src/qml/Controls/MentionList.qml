pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Templates as T

Item {
    id: root

    property var suggestions: []
    property int highlighted: 0
    property Look look: KodosiTheme.look
    property string identifier: ""

    signal picked(var target)
    signal menuRequested(var target, Item row, point position)

    implicitHeight: list.implicitHeight + 10

    Raised { anchors.fill: parent; radius: KodosiTheme.radiusLg; look: root.look; fill: root.look.lifted; elevation: 2 }
    Column {
        id: list
        x: 5
        y: 5
        width: parent.width - 10
        spacing: 1

        Repeater {
            model: root.suggestions

            AbstractButton {
                id: suggestion

                required property var modelData
                required property int index
                readonly property bool current: index === Math.min(root.highlighted, root.suggestions.length - 1)

                width: list.width
                height: 32
                hoverEnabled: true
                focusPolicy: Qt.NoFocus
                objectName: root.identifier + "." + modelData.id
                Accessible.id: objectName
                Accessible.name: modelData.name
                contentItem: RowLayout {
                    spacing: 9
                    Item {
                        Layout.leftMargin: 8
                        Layout.preferredWidth: 22
                        Layout.preferredHeight: 22
                        PersonAvatar { visible: suggestion.modelData.kind === "person"; name: suggestion.modelData.name; key: suggestion.modelData.id; size: 22 }
                        AgentMark { visible: suggestion.modelData.kind === "terminal"; program: suggestion.modelData.program; size: 22 }
                    }
                    PlainLabel { text: suggestion.modelData.name; color: root.look.ink; font.weight: Font.Medium }
                    PlainLabel { Layout.fillWidth: true; text: suggestion.modelData.detail; color: root.look.inkFaint; font.pixelSize: KodosiTheme.fontCaption; elide: Text.ElideRight }
                }
                background: Rectangle {
                    radius: KodosiTheme.radiusSm
                    color: suggestion.current || suggestion.hovered ? KodosiTheme.mix(root.look.lifted, root.look.ink, 0.06) : "transparent"
                }
                onClicked: root.picked(modelData)
                T.ContextMenu.onRequested: position => {
                    const list = root
                    const row = suggestion
                    const target = modelData
                    // A menu takes the focus, which can rebuild the list while this row still handles its
                    // event, and Qt crashes when a row goes away during it.
                    Qt.callLater(() => list.menuRequested(target, row, position))
                }
            }
        }
    }
}
