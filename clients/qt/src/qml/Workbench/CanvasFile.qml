import Kodosi 1.0
import QtQuick
import Kodosi.Models 1.0 as Models

// A canvas file that this app does not show itself: its name and size, and the way to open it.
Item {
    id: root

    required property string sessionId
    required property var item
    property bool local: false

    function size(bytes) {
        return bytes >= 1048576 ? qsTr("%1 MB").arg((bytes / 1048576).toFixed(1))
            : bytes >= 1024 ? qsTr("%1 KB").arg(Math.round(bytes / 1024)) : qsTr("%1 bytes").arg(bytes);
    }

    EmptyState {
        anchors.centerIn: parent
        width: Math.min(340, parent.width - 32)
        title: root.item.name
        message: (root.item.kind === "page" ? qsTr("Page") : root.item.kind === "pdf" ? qsTr("PDF")
            : root.item.kind === "sheet" ? qsTr("Sheet") : root.item.kind === "presentation" ? qsTr("Presentation") : qsTr("Document"))
            + " · " + root.size(root.item.bytes || 0)
            + (["document", "sheet", "presentation"].includes(root.item.kind) ? "" : "\n" + qsTr("This computer does not let Kodosi show pages in a sandbox."))
        art: IconTile {
            iconName: ["document", "sheet", "presentation"].includes(root.item.kind) ? root.item.kind : "page"
            size: 56
        }

        KButton {
            visible: root.local
            text: qsTr("Open in its app")
            iconName: "open"
            variant: KButton.Primary
            objectName: "canvas.file.open"
            Accessible.id: objectName
            onClicked: Models.DesktopFiles.openPath(root.item.key)
        }
        KButton {
            text: qsTr("Save a copy")
            iconName: "save"
            objectName: "canvas.file.save"
            Accessible.id: objectName
            onClicked: Models.Canvas.saveCopy(root.sessionId, root.item.key)
        }
    }
}
