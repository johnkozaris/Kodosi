pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

Item {
    id: root

    property string html: ""
    property string text: ""
    property var parts: []
    property var handoff: null
    property int pictures: 0
    property Look look: KodosiTheme.look
    readonly property real widest: Math.max(0, width - 56 - 24)

    signal linkActivated(string link)

    implicitHeight: bubble.height
    Accessible.role: Accessible.StaticText
    Accessible.name: (handed.item as ChatHandOff)?.words ?? root.text

    Rectangle {
        id: bubble

        anchors.right: parent.right
        width: content.width + 24
        height: content.height + 16
        radius: 16
        bottomRightRadius: 6
        color: root.look.accentSoft

        Column {
            id: content

            x: 12
            y: 8
            spacing: 8

            MentionText {
                id: words

                visible: !handed.active && root.text.length > 0
                width: Math.min(implicitWidth, root.widest)
                text: Prose.styled(root.html, root.look)
                parts: root.parts
                look: root.look
                Accessible.ignored: true
                onLinkActivated: link => root.linkActivated(link)
            }
            Loader {
                id: handed

                visible: active
                active: !!root.handoff
                sourceComponent: ChatHandOff {
                    handoff: root.handoff
                    look: root.look
                    widest: root.widest
                }
            }
            ChatPictures {
                count: root.pictures
                look: root.look
            }
        }
    }
}
