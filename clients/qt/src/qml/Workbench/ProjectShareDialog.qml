import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    property string sessionId: ""
    property string projectId: ""
    readonly property var session: Models.Sessions.presentationForSession(sessionId)
    readonly property string projectName: nameOf(projectId)
    readonly property string leaves: session.projectId && session.projectId !== projectId ? nameOf(session.projectId) : ""

    signal shared(string projectId)

    function nameOf(id) {
        return (Models.Projects.projects.find(entry => entry.id === id) || {}).name || qsTr("the project")
    }
    function ask(id, target) {
        const entry = Models.Sessions.presentationForSession(id)
        if (entry.isOwner !== true || entry.projectId === target)
            return false
        sessionId = id
        projectId = target
        open()
        return true
    }

    standardButtons: Dialog.Ok | Dialog.Cancel
    title: qsTr("Share %1 with %2?").arg(session.name || qsTr("this terminal")).arg(projectName)
    objectName: "project.share.ask"
    onOpened: standardButton(Dialog.Ok).text = qsTr("Share")
    onAccepted: {
        if (Models.SessionActions.attachProject(sessionId, projectId))
            root.shared(projectId)
    }

    PlainLabel {
        width: 340
        text: root.leaves.length > 0
            ? qsTr("Everyone in %1 gets full control of it. It leaves %2.").arg(root.projectName).arg(root.leaves)
            : qsTr("Everyone in %1 gets full control of it.").arg(root.projectName)
        color: KodosiTheme.inkMuted
        wrapMode: Text.WordWrap
    }
}
