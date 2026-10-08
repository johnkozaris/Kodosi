import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

KPopover {
    id: root

    readonly property bool contentAvailable: root.visible
    readonly property bool failed: Models.ApplicationLifecycle.state === Models.ApplicationLifecycle.Failed

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

    background: Rectangle {
        color: KodosiTheme.canvas
    }
    contentItem: FocusScope {
        id: startupScope

        Accessible.description: root.failed ? Models.ApplicationLifecycle.errorText : qsTr("Kodosi is starting.")
        Accessible.id: objectName
        Accessible.ignored: !root.visible
        Accessible.name: root.failed ? qsTr("Kodosi couldn’t start") : qsTr("Starting Kodosi")
        Accessible.role: Accessible.Dialog
        focus: true
        objectName: "startup.overlay"

        ColumnLayout {
            id: startupContent

            Accessible.id: objectName
            Accessible.ignored: !root.contentAvailable
            anchors.centerIn: parent
            enabled: root.contentAvailable
            objectName: "startup.content"
            spacing: KodosiTheme.spacing5
            visible: root.contentAvailable
            width: Math.min(460, Math.max(280, parent.width - 48))

            Item {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredHeight: 56
                Layout.preferredWidth: 56

                Rectangle {
                    anchors.fill: parent
                    color: KodosiTheme.surfaceElevated
                    radius: KodosiTheme.radiusLarge
                }
                KBusyIndicator {
                    id: busyIndicator

                    Accessible.id: objectName
                    Accessible.ignored: true
                    anchors.centerIn: parent
                    objectName: "startup.busy"
                    running: root.contentAvailable && !root.failed
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
                    visible: root.contentAvailable && root.failed
                    width: 28
                }
            }
            PlainLabel {
                id: titleLabel

                Accessible.id: objectName
                Accessible.ignored: !root.contentAvailable
                Accessible.name: text
                Accessible.role: Accessible.StaticText
                Layout.fillWidth: true
                color: root.failed ? KodosiTheme.textPrimary : KodosiTheme.textSecondary
                font.pixelSize: root.failed ? 20 : 14
                font.weight: root.failed ? Font.DemiBold : Font.Medium
                horizontalAlignment: Text.AlignHCenter
                objectName: "startup.title"
                text: root.failed ? qsTr("Kodosi couldn’t start") : qsTr("Starting Kodosi…")
                wrapMode: Text.Wrap
            }
            PlainLabel {
                id: detailLabel

                Accessible.id: objectName
                Accessible.ignored: !root.contentAvailable
                Accessible.name: text
                Accessible.role: Accessible.StaticText
                Layout.fillWidth: true
                Layout.topMargin: root.failed ? 0 : -KodosiTheme.spacing2
                color: KodosiTheme.textSecondary
                elide: Text.ElideRight
                font.pixelSize: 12
                horizontalAlignment: Text.AlignHCenter
                maximumLineCount: 5
                objectName: "startup.failure.detail"
                text: Models.ApplicationLifecycle.errorText
                visible: root.contentAvailable && root.failed
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: root.failed ? KodosiTheme.spacing3 : 0
                spacing: KodosiTheme.spacing3
                visible: root.contentAvailable && root.failed

                KButton {
                    id: retryButton

                    Accessible.id: objectName
                    Accessible.ignored: !root.contentAvailable
                    Accessible.name: text
                    iconName: "refresh"
                    objectName: "startup.retry"
                    text: qsTr("Try Again")
                    variant: KButton.Primary

                    onClicked: Models.ApplicationLifecycle.retry()
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
