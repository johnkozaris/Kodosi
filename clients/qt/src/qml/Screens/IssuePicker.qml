pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property var repositories: Models.Missions.room.repositories || []
    readonly property var tasks: Models.Missions.room.tasks || []
    readonly property string words: search.text.trim().toLowerCase()
    readonly property int number: {
        const match = /^#?(\d{1,9})$/.exec(words)
        return match ? Number(match[1]) : 0
    }
    readonly property var rows: {
        const result = []
        for (const repository of repositories) {
            result.push({ header: true, name: repository.name, id: repository.id })
            const issues = (Models.Missions.issuesByRepository[repository.id] || [])
                .filter(issue => !issue.closed && (words.length === 0 || issue.title.toLowerCase().indexOf(words) >= 0 || ("#" + issue.number).indexOf(words) >= 0))
            for (const issue of issues)
                result.push(Object.assign({ repositoryId: repository.id }, issue))
            if (number > 0 && !issues.some(issue => issue.number === number))
                result.push({ repositoryId: repository.id, number: number, title: qsTr("Add issue #%1").arg(number), byNumber: true })
        }
        return result
    }

    function load() {
        for (const repository of repositories)
            Models.Missions.roomAction({ type: "issues", repositoryId: repository.id })
    }

    spacing: 8
    Component.onCompleted: load()

    PlainLabel {
        visible: root.repositories.length === 0
        text: qsTr("Link a repository")
        font.weight: Font.DemiBold
    }
    Loader {
        Layout.fillWidth: true
        active: root.repositories.length === 0
        visible: active
        sourceComponent: RepositoryPicker {}
    }
    Item {
        Layout.fillWidth: true
        implicitHeight: 36
        visible: root.repositories.length > 0

        Well { anchors.fill: parent; radius: height / 2 }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 8

            KIcon { Layout.preferredWidth: 13; Layout.preferredHeight: 13; name: "search"; color: KodosiTheme.inkFaint }
            TextInput {
                id: search
                Layout.fillWidth: true
                color: KodosiTheme.ink
                font.pixelSize: KodosiTheme.fontBody
                selectionColor: KodosiTheme.accent
                selectedTextColor: KodosiTheme.accentInk
                clip: true
                objectName: "room.issue.find"
                Accessible.id: objectName
                Accessible.name: qsTr("Find an issue")
                Accessible.role: Accessible.EditableText

                PlainLabel { visible: search.text.length === 0; text: qsTr("Find an issue"); color: KodosiTheme.inkFaint }
            }
        }
    }
    ListView {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(contentHeight, 360)
        visible: root.repositories.length > 0
        clip: true
        spacing: 1
        model: root.rows
        ScrollBar.vertical: KScrollBar {}
        delegate: AbstractButton {
            id: row

            required property var modelData
            readonly property bool imported: !modelData.header && root.tasks.some(task => task.issue && task.issue.url === modelData.url)

            width: ListView.view.width
            height: modelData.header ? 28 : 34
            hoverEnabled: !modelData.header
            enabled: !modelData.header && !imported
            Accessible.name: modelData.header ? modelData.name : imported ? qsTr("%1 is a task").arg(modelData.title) : qsTr("Add %1 to tasks").arg(modelData.title)
            contentItem: RowLayout {
                spacing: 10
                KIcon {
                    Layout.leftMargin: 8
                    Layout.preferredWidth: row.modelData.header ? 11 : 13
                    Layout.preferredHeight: row.modelData.header ? 11 : 13
                    name: row.modelData.header ? "repository" : "issue"
                    color: row.modelData.header ? KodosiTheme.inkFaint : row.modelData.byNumber ? KodosiTheme.inkMuted : KodosiTheme.ready
                }
                PlainLabel {
                    Layout.fillWidth: true
                    text: row.modelData.header ? row.modelData.name : row.modelData.title || ""
                    color: row.modelData.header ? KodosiTheme.inkFaint : KodosiTheme.ink
                    font.pixelSize: row.modelData.header ? KodosiTheme.fontCaption : KodosiTheme.fontBody
                    elide: Text.ElideRight
                }
                PlainLabel {
                    visible: !row.modelData.header && !row.modelData.byNumber
                    text: "#" + (row.modelData.number || "")
                    color: KodosiTheme.inkFaint
                    font.pixelSize: KodosiTheme.fontCaption
                }
                KIcon {
                    visible: !row.modelData.header
                    Layout.rightMargin: 8
                    Layout.preferredWidth: 12
                    Layout.preferredHeight: 12
                    name: row.imported ? "check" : "plus"
                    color: row.imported ? KodosiTheme.ready : KodosiTheme.inkMuted
                }
            }
            background: Rectangle {
                radius: KodosiTheme.radiusMd
                color: KodosiTheme.alpha(KodosiTheme.ink, row.hovered && row.enabled ? 0.06 : 0)
            }
            onClicked: {
                Models.Missions.roomAction({ type: "importIssue", repositoryId: modelData.repositoryId, number: modelData.number })
                if (modelData.byNumber)
                    search.clear()
            }
        }
    }
}
