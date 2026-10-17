import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    readonly property bool failed: Models.ApplicationLifecycle.state === Models.ApplicationLifecycle.Failed
    readonly property bool outdated: failed && Models.ApplicationLifecycle.hostOutdated

    function restorePrimaryFocus() {
        if (!visible)
            return;
        if (failed)
            retryButton.forceActiveFocus(Qt.PopupFocusReason);
        else
            startupScope.forceActiveFocus(Qt.PopupFocusReason);
    }

    closePolicy: Popup.NoAutoClose
    dim: false
    enabled: visible
    focus: visible
    height: parent ? parent.height : 0
    modal: true
    objectName: "startup.overlay.popup"
    padding: 0
    parent: Overlay.overlay
    visible: Models.ApplicationLifecycle.state !== Models.ApplicationLifecycle.Ready
    width: parent ? parent.width : 0
    x: 0
    y: 0

    radius: 0
    enter: null
    background: Rectangle {
        color: KodosiTheme.ground
    }
    contentItem: FocusScope {
        id: startupScope

        Accessible.description: root.failed ? Models.ApplicationLifecycle.errorText : qsTr("Kodosi is starting.")
        Accessible.id: objectName
        Accessible.ignored: !root.visible
        Accessible.name: root.outdated ? qsTr("Kodosi was updated") : root.failed ? qsTr("Kodosi could not start") : qsTr("Starting Kodosi")
        Accessible.role: Accessible.Dialog
        focus: true
        objectName: "startup.overlay"

        ColumnLayout {
            Accessible.id: objectName
            anchors.centerIn: parent
            objectName: "startup.content"
            spacing: 12
            width: Math.min(460, Math.max(280, parent.width - 48))

            Item {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredHeight: 56
                Layout.preferredWidth: Math.max(56, mark.implicitWidth)

                Wordmark {
                    id: mark
                    anchors.centerIn: parent
                    size: 24
                    blinks: true
                    visible: !root.failed
                    objectName: "startup.busy"
                    Accessible.id: objectName
                }
                Well {
                    anchors.centerIn: parent
                    width: 56
                    height: 56
                    visible: root.failed
                }
                KIcon {
                    Accessible.id: objectName
                    Accessible.ignored: true
                    anchors.centerIn: parent
                    color: KodosiTheme.danger
                    height: 28
                    name: "warning"
                    objectName: "startup.failure.icon"
                    strokeWidth: 1.9
                    visible: root.failed
                    width: 28
                }
            }
            PlainLabel {
                Accessible.id: objectName
                Accessible.name: text
                Accessible.role: Accessible.StaticText
                Layout.fillWidth: true
                font.pixelSize: KodosiTheme.fontTitle
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
                objectName: "startup.title"
                text: root.outdated ? qsTr("Kodosi was updated") : qsTr("Kodosi could not start")
                visible: root.failed
                wrapMode: Text.Wrap
            }
            PlainLabel {
                Accessible.id: objectName
                Accessible.name: text
                Accessible.role: Accessible.StaticText
                Layout.fillWidth: true
                color: KodosiTheme.inkMuted
                elide: Text.ElideRight
                font.pixelSize: 12
                horizontalAlignment: Text.AlignHCenter
                maximumLineCount: 5
                objectName: "startup.failure.detail"
                text: Models.ApplicationLifecycle.errorText
                visible: root.failed
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 8
                spacing: 8
                visible: root.failed

                KButton {
                    id: retryButton

                    Accessible.id: objectName
                    Accessible.name: text
                    iconName: "refresh"
                    objectName: "startup.retry"
                    text: qsTr("Try again")
                    variant: root.outdated && !Models.ApplicationLifecycle.hostForeground ? KButton.Secondary : KButton.Primary

                    onClicked: Models.ApplicationLifecycle.retry()
                }
                KButton {
                    Accessible.id: objectName
                    Accessible.name: text
                    objectName: "startup.restartHost"
                    text: Models.ApplicationLifecycle.hostTerminals === 0 ? qsTr("Restart Kodosi") : qsTr("Restart it and end its terminals")
                    variant: KButton.Primary
                    visible: root.outdated && !Models.ApplicationLifecycle.hostForeground

                    onClicked: Models.ApplicationLifecycle.restartHost()
                }
            }
        }
    }

    onFailedChanged: Qt.callLater(restorePrimaryFocus)
    onVisibleChanged: {
        if (visible)
            Qt.callLater(restorePrimaryFocus);
    }
}
