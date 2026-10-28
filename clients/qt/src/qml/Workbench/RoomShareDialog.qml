import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property string sessionId: ""
    property string roomId: ""
    readonly property var session: Models.Sessions.presentationForSession(sessionId)
    readonly property string roomName: room(roomId)
    readonly property string leaves: session.missionId && session.missionId !== roomId ? room(session.missionId) : ""

    signal shared(string roomId)

    function room(id) {
        return (Models.Missions.missions.find(entry => entry.id === id) || {}).name || qsTr("the room")
    }
    function ask(id, room) {
        const entry = Models.Sessions.presentationForSession(id)
        if (entry.isOwner !== true || entry.missionId === room)
            return false
        sessionId = id
        roomId = room
        open()
        return true
    }

    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Share %1 with %2?").arg(session.name || qsTr("this terminal")).arg(roomName)
    objectName: "room.share.ask"
    onOpened: standardButton(Dialog.Ok).text = qsTr("Share")
    onAccepted: {
        if (Models.SessionActions.attachMission(sessionId, roomId))
            root.shared(roomId)
    }

    PlainLabel {
        width: 340
        text: root.leaves.length > 0
            ? qsTr("Everyone in %1 gets full control of it. It leaves %2.").arg(root.roomName).arg(root.leaves)
            : qsTr("Everyone in %1 gets full control of it.").arg(root.roomName)
        color: KodosiTheme.inkMuted
        wrapMode: Text.WordWrap
    }
}
