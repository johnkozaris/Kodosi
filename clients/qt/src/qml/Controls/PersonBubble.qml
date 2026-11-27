pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string html: ""
    property string text: ""
    property var parts: []
    property var handoff: null
    property Look look: KodosiTheme.look
    readonly property real widest: Math.max(0, width - 56 - 24)

    signal linkActivated(string link)

    implicitHeight: bubble.height
    Accessible.role: Accessible.StaticText
    Accessible.name: (handed.item as ChatHandOff)?.words ?? root.text

    Rectangle {
        id: bubble

        anchors.right: parent.right
        width: (handed.active ? handed.width : Math.min(words.implicitWidth, root.widest)) + 24
        height: (handed.active ? handed.height : words.implicitHeight) + 16
        radius: 16
        bottomRightRadius: 6
        color: root.look.accentSoft

        MentionText {
            id: words

            x: 12
            y: 8
            visible: !handed.active
            width: Math.min(implicitWidth, root.widest)
            text: Prose.styled(root.html, root.look)
            parts: root.parts
            look: root.look
            Accessible.ignored: true
            onLinkActivated: link => root.linkActivated(link)
        }
        Loader {
            id: handed

            x: 12
            y: 8
            active: !!root.handoff
            sourceComponent: ChatHandOff {
                handoff: root.handoff
                look: root.look
                widest: root.widest
            }
        }
    }
}
