pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

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
                : Models.Canvas.browserGetting >= 0 ? qsTr("Downloading, %1%").arg(Models.Canvas.browserGetting)
                : Models.Canvas.browserEngine.length > 0
                    ? [Models.Canvas.browserEngine].concat(Models.Canvas.browserDone["engine"] ?? []).join(" · ")
                : qsTr("Downloads when the shared browser first opens")

            KButton {
                Accessible.id: objectName
                compact: true
                visible: Models.Canvas.browserKnown && Models.Canvas.browserGetting < 0
                enabled: !root.busy
                working: Models.Canvas.browserWork === "engine"
                objectName: "settings.browser.get"
                text: Models.Canvas.browserEngine.length > 0 ? qsTr("Check for updates") : qsTr("Download")

                onClicked: Models.Canvas.getBrowser()
            }
        }
    }
    Timer {
        interval: 400
        repeat: true
        running: root.visible && (Models.Canvas.browserWork === "engine" || Models.Canvas.browserGetting >= 0)

        onTriggered: Models.Canvas.refreshBrowser()
    }
    ListGroup {
        Layout.fillWidth: true
        footer: qsTr("Kodosi Browser starts again when you change one of these, and its open pages load again.")

        ListRow {
            id: formPredictions

            iconName: "text"
            tint: "#5fa57e"
            title: qsTr("Form predictions from Google")
            subtitle: qsTr("Sends Google a code of each form to learn what its fields are for")

            KSwitch {
                Accessible.id: objectName
                Accessible.name: formPredictions.title
                checked: Models.Canvas.browserFormPredictions
                enabled: Models.Canvas.browserKnown && !root.busy
                objectName: "settings.browser.formPredictions"

                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => Models.Canvas.browserFormPredictions);
                    Models.Canvas.setBrowserPrivacy(wanted, Models.Canvas.browserCrossSiteCookies);
                }
            }
        }
        ListRow {
            id: crossSiteCookies

            iconName: "eye"
            tint: "#8c7bd1"
            title: qsTr("Cross-site cookies")
            subtitle: qsTr("Sites shown inside a page, such as YouTube or Google, know you there")

            KSwitch {
                Accessible.id: objectName
                Accessible.name: crossSiteCookies.title
                checked: Models.Canvas.browserCrossSiteCookies
                enabled: Models.Canvas.browserKnown && !root.busy
                objectName: "settings.browser.crossSiteCookies"

                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => Models.Canvas.browserCrossSiteCookies);
                    Models.Canvas.setBrowserPrivacy(Models.Canvas.browserFormPredictions, wanted);
                }
            }
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
