pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The live viewers of this device: which ones show their files on the canvas, and how.
ColumnLayout {
    id: root

    readonly property var viewers: Models.DesktopSettings.viewers

    spacing: 18

    component Choice: ListRow {
        id: choice

        required property string name

        KSwitch {
            Accessible.id: objectName
            Accessible.name: choice.title
            checked: Models.DesktopSettings.viewers[choice.name] === true
            objectName: "settings.documents." + choice.name

            onToggled: {
                const wanted = checked;
                checked = Qt.binding(() => Models.DesktopSettings.viewers[choice.name] === true);
                Models.DesktopSettings.setViewer(choice.name, wanted);
            }
        }
    }

    ListGroup {
        Layout.fillWidth: true

        Choice {
            name: "documents"
            iconName: "document"
            tint: "#5b82c9"
            title: qsTr("Document viewer")
            subtitle: ".docx"
        }
        Choice {
            visible: root.viewers.documents === true
            name: "documentPages"
            iconName: "files"
            tint: "#4f9bb0"
            title: qsTr("Pages")
            subtitle: qsTr("When off, the text fits the canvas")
        }
        Choice {
            visible: root.viewers.documents === true
            name: "darkDocuments"
            iconName: "moon"
            tint: "#6b6560"
            title: qsTr("Dark pages")
            subtitle: qsTr("When Kodosi is dark")
        }
    }
    ListGroup {
        Layout.fillWidth: true

        Choice {
            name: "sheets"
            iconName: "sheet"
            tint: "#5fa57e"
            title: qsTr("Sheet viewer")
            subtitle: ".xlsx"
        }
        Choice {
            visible: root.viewers.sheets === true
            name: "darkSheets"
            iconName: "moon"
            tint: "#6b6560"
            title: qsTr("Dark sheets")
            subtitle: qsTr("When Kodosi is dark")
        }
    }
    ListGroup {
        Layout.fillWidth: true

        Choice {
            name: "presentations"
            iconName: "presentation"
            tint: "#d9a441"
            title: qsTr("Presentation viewer")
            subtitle: ".pptx"
        }
    }
    ListGroup {
        Layout.fillWidth: true

        Choice {
            name: "texts"
            iconName: "pencil"
            tint: "#6b6560"
            title: qsTr("Text viewer")
            subtitle: ".txt, .json, .yaml, .toml, .xml"
        }
    }
    ListGroup {
        Layout.fillWidth: true
        footer: qsTr("When a viewer is off, you save a copy of its files or open them in their own app.")

        Choice {
            name: "pdfs"
            iconName: "page"
            tint: "#cf6a62"
            title: qsTr("PDF viewer")
            subtitle: ".pdf"
        }
        Choice {
            visible: root.viewers.pdfs === true
            name: "darkPdfs"
            iconName: "moon"
            tint: "#6b6560"
            title: qsTr("Dark pages")
            subtitle: qsTr("When Kodosi is dark")
        }
    }
    PlainLabel {
        Layout.fillWidth: true
        visible: text.length > 0
        color: KodosiTheme.danger
        font.pixelSize: KodosiTheme.fontFootnote
        text: Models.DesktopSettings.settingsError
        wrapMode: Text.WordWrap
    }
}
