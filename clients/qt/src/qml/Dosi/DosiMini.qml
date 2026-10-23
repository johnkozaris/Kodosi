pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

Item {
    id: root

    property var tool: ({})
    readonly property string kind: tool.targetKind || ""
    readonly property var session: kind === "terminal" && tool.sessionId ? Models.Sessions.presentationForSession(tool.sessionId) : null
    readonly property var stack: kind === "terminals" ? Models.Sessions.listedSessions.slice(0, 3)
        : kind === "rooms" ? Models.Missions.missions.slice(0, 3) : []
    readonly property string glyph: kind === "terminal" ? "terminal" : kind === "url" ? "link" : kind === "file" ? "document"
        : kind === "query" ? "search" : kind === "command" ? "prompt" : tool.icon || "cube"

    implicitWidth: stack.length > 0 ? 14 + (stack.length - 1) * 9 : 14
    implicitHeight: 14
    Accessible.ignored: true

    Loader {
        anchors.fill: parent
        sourceComponent: root.session && root.session.id ? terminal
            : root.kind === "room" ? room
            : root.stack.length > 0 ? pile
            : root.kind === "agent" ? agent : symbol
    }
    Component {
        id: terminal
        AgentMark {
            size: 14
            program: root.session.program || ""
            session: root.session
        }
    }
    Component {
        id: room
        RoomSigil {
            size: 14
            key: root.tool.roomId || root.tool.targetValue || ""
        }
    }
    Component {
        id: agent
        AgentMark {
            size: 14
            program: root.tool.targetValue || ""
        }
    }
    Component {
        id: pile
        Item {
            Repeater {
                model: root.stack

                Item {
                    id: entry

                    required property var modelData
                    required property int index

                    x: index * 9
                    width: 14
                    height: 14
                    z: -index

                    AgentMark {
                        visible: root.kind === "terminals"
                        size: 14
                        program: entry.modelData.program || ""
                        session: root.kind === "terminals" ? entry.modelData : null
                    }
                    RoomSigil {
                        visible: root.kind === "rooms"
                        size: 14
                        key: entry.modelData.id || ""
                    }
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -1.5
                        radius: 5.5
                        color: "transparent"
                        border.width: 1.5
                        border.color: KodosiTheme.ground
                    }
                }
            }
        }
    }
    Component {
        id: symbol
        Rectangle {
            radius: 4
            color: KodosiTheme.lifted

            KIcon {
                anchors.centerIn: parent
                width: 9
                height: 9
                name: root.glyph
                color: KodosiTheme.inkMuted
                strokeWidth: 2.4
            }
        }
    }
}
