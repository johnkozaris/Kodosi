pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

ColumnLayout {
    id: root

    readonly property var choices: Models.Missions.repositoryChoices
    readonly property var linked: (Models.Missions.room.repositories || []).map(repository => repository.url.toLowerCase())
    readonly property string words: search.text.trim()
    readonly property bool looksLikeAddress: words.length > 0 && words.indexOf(" ") < 0
        && (words.indexOf("://") >= 0 || words.startsWith("git@") || words.split("/").length > 2)
    readonly property var shown: (choices.repositories || []).filter(choice => words.length === 0 || choice.name.toLowerCase().indexOf(words.toLowerCase()) >= 0)

    signal done()

    function add(url) {
        if (root.linked.indexOf(url.toLowerCase()) >= 0)
            return
        Models.Missions.roomAction({ type: "addRepository", url: url })
        search.clear()
        root.done()
    }
    function submit() {
        if (looksLikeAddress)
            add(words)
        else if (shown.length === 1)
            add(shown[0].url)
    }

    spacing: 8
    Component.onCompleted: {
        Models.Missions.listRepositories()
        search.forceActiveFocus()
    }

    Item {
        Layout.fillWidth: true
        implicitHeight: 36

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
                objectName: "room.repository.find"
                Accessible.id: objectName
                Accessible.name: qsTr("Find or paste a repository")
                Accessible.role: Accessible.EditableText
                onAccepted: root.submit()
                Keys.onEscapePressed: root.done()

                PlainLabel { visible: search.text.length === 0; text: qsTr("Find or paste a repository"); color: KodosiTheme.inkFaint }
            }
        }
    }
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 40
        visible: root.choices.github === "signedOut" || root.choices.github === "missing"
        radius: KodosiTheme.radiusMd
        color: KodosiTheme.alpha(KodosiTheme.accentSoft, 0.5)

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 6
            spacing: 10

            KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "people"; color: KodosiTheme.accentStrong }
            PlainLabel {
                Layout.fillWidth: true
                text: root.choices.github === "missing" ? qsTr("Install GitHub CLI to see your repositories") : qsTr("Connect GitHub to see your repositories")
                color: KodosiTheme.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                elide: Text.ElideRight
            }
            KButton {
                compact: true
                variant: KButton.Tinted
                text: root.choices.github === "missing" ? qsTr("Get it") : qsTr("Connect")
                objectName: "room.github.connect"
                Accessible.id: objectName
                onClicked: {
                    if (root.choices.github === "missing") {
                        Qt.openUrlExternally("https://cli.github.com")
                        return
                    }
                    Models.SessionActions.create(qsTr("GitHub sign-in"), Models.DesktopSettings.effectiveWorkingDirectory, "gh auth login")
                    root.done()
                }
            }
        }
    }
    ListView {
        id: list
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(contentHeight, 300)
        clip: true
        spacing: 1
        model: (root.looksLikeAddress ? [{ name: root.words, url: root.words, address: true }] : []).concat(root.shown)
        ScrollBar.vertical: KScrollBar {}
        delegate: AbstractButton {
            id: row

            required property var modelData
            readonly property bool done: root.linked.indexOf((modelData.url || "").toLowerCase()) >= 0

            width: ListView.view.width
            height: modelData.folder ? 44 : 36
            hoverEnabled: true
            enabled: !done
            objectName: "room.repository.choice." + (modelData.url || "")
            Accessible.id: objectName
            Accessible.name: modelData.name || ""
            contentItem: RowLayout {
                spacing: 10
                IconTile {
                    Layout.leftMargin: 8
                    iconName: row.modelData.address ? "link" : row.modelData.folder ? "folder" : "repository"
                    tint: row.modelData.address ? "#5b82c9" : row.modelData.folder ? "#4f9bb0" : "#8c7bd1"
                    size: 24
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1
                    PlainLabel { Layout.fillWidth: true; text: row.modelData.name || ""; font.weight: Font.Medium; elide: Text.ElideRight }
                    PlainLabel {
                        Layout.fillWidth: true
                        visible: !!row.modelData.folder
                        text: row.modelData.folder || ""
                        color: KodosiTheme.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption
                        elide: Text.ElideMiddle
                    }
                }
                KIcon { visible: row.modelData.private === true; Layout.preferredWidth: 11; Layout.preferredHeight: 11; name: "lock"; color: KodosiTheme.inkFaint }
                KIcon { visible: row.done; Layout.rightMargin: 8; Layout.preferredWidth: 12; Layout.preferredHeight: 12; name: "check"; color: KodosiTheme.ready }
            }
            background: Rectangle {
                radius: KodosiTheme.radiusMd
                color: KodosiTheme.alpha(KodosiTheme.ink, row.hovered ? 0.06 : 0)
            }
            onClicked: root.add(modelData.url)
        }
    }
    Spinner {
        Layout.alignment: Qt.AlignHCenter
        visible: root.choices.loading === true
    }
    PlainLabel {
        Layout.fillWidth: true
        visible: root.choices.loading !== true && list.count === 0
        text: root.choices.failure || (root.words.length > 0 ? qsTr("No match") : qsTr("No repositories found"))
        color: root.choices.failure ? KodosiTheme.danger : KodosiTheme.inkFaint
        font.pixelSize: KodosiTheme.fontFootnote
        wrapMode: Text.WordWrap
        leftPadding: 10
    }
}
