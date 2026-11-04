pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The shared browser of this computer: its Kodosi Browser, the sign-ins that a person copies
// from another browser, and the data of the private terminals. Each step is a person's own.
ColumnLayout {
    id: root

    readonly property bool busy: Models.Canvas.browserWork.length > 0

    spacing: 18

    onVisibleChanged: {
        if (visible)
            Models.Canvas.openBrowserSettings();
    }

    ListGroup {
        Layout.fillWidth: true

        ListRow {
            iconName: "globe"
            tint: "#5b82c9"
            title: qsTr("Kodosi Browser")
            subtitle: !Models.Canvas.browserKnown ? qsTr("Looking on this computer")
                : Models.Canvas.browserEngine.length > 0 ? Models.Canvas.browserEngine
                : qsTr("Downloads when the shared browser first opens")
        }
    }
    ListGroup {
        Layout.fillWidth: true
        footer: [qsTr("Sites then know you in your private terminals on this computer. Passkeys and sign-ins bound to a device stay in their own browser.")]
            .concat(Qt.platform.os === "windows" ? [qsTr("Chrome keeps its sign-ins locked on Windows, so sign in on the canvas.")] : [])
            .join(" ")

        ListRow {
            visible: Models.Canvas.browserKnown && Models.Canvas.browserSources.length === 0
            iconName: "globe"
            tint: "#6b6560"
            title: qsTr("No other browser on this computer")
        }
        Repeater {
            model: Models.Canvas.browserSources

            ListRow {
                id: source

                required property string modelData

                iconName: "globe"
                tint: "#4f9bb0"
                title: modelData
                subtitle: Models.Canvas.browserDone[modelData] ?? ""

                KButton {
                    Accessible.id: objectName
                    compact: true
                    enabled: !root.busy
                    working: Models.Canvas.browserWork === source.modelData
                    objectName: "settings.browser.copy." + source.modelData
                    text: qsTr("Copy")

                    onClicked: Models.Canvas.copySignIns(source.modelData)
                }
            }
        }
    }
    ListGroup {
        Layout.fillWidth: true

        ListRow {
            iconName: "key"
            tint: "#d9a441"
            title: qsTr("Copied sign-ins")
            subtitle: Models.Canvas.browserSites > 0 ? qsTr("For %n sites", "", Models.Canvas.browserSites) : qsTr("None")

            KButton {
                Accessible.id: objectName
                compact: true
                enabled: !root.busy && Models.Canvas.browserSites > 0
                working: Models.Canvas.browserWork === "forget"
                objectName: "settings.browser.forget"
                text: qsTr("Remove")

                onClicked: Models.Canvas.forgetSignIns()
            }
        }
        ListRow {
            iconName: "trash"
            tint: "#cf6a62"
            title: qsTr("Browsing data")
            subtitle: Models.Canvas.browserDone["clear"] ?? qsTr("Sign-ins and site data of your private terminals")

            KButton {
                Accessible.id: objectName
                compact: true
                variant: KButton.Danger
                enabled: !root.busy
                working: Models.Canvas.browserWork === "clear"
                objectName: "settings.browser.clear"
                text: qsTr("Remove all…")

                onClicked: clearConfirmation.open()
            }
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        visible: Models.Canvas.browserProblem.length > 0
        text: Models.Canvas.browserProblem
        color: KodosiTheme.ink
        wrapMode: Text.WordWrap
        padding: 10
        leftPadding: 12
        background: Rectangle {
            color: KodosiTheme.cautionSoft
            radius: KodosiTheme.radiusMd
        }
    }
    KDialog {
        id: clearConfirmation

        destructive: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        title: qsTr("Remove all browsing data?")
        onOpened: standardButton(Dialog.Ok).text = qsTr("Remove all")

        onAccepted: Models.Canvas.removeBrowsingData()

        PlainLabel {
            color: KodosiTheme.inkMuted
            text: qsTr("Sites forget you in your private terminals on this computer. Their open pages load again.")
            width: 380
            wrapMode: Text.WordWrap
        }
    }
}
