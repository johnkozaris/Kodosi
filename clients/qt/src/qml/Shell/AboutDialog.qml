import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

KDialog {
    id: root

    objectName: "about"
    standardButtons: Dialog.Close

    ColumnLayout {
        implicitWidth: 300
        spacing: 10

        Wordmark {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: 18
            size: 30
        }
        PlainLabel {
            Layout.alignment: Qt.AlignHCenter
            Layout.bottomMargin: 6
            text: qsTr("Version %1").arg(Qt.application.version)
            color: KodosiTheme.inkMuted
            font.pixelSize: KodosiTheme.fontFootnote
        }
    }
}
