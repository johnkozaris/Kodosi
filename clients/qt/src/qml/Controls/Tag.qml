import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root

    enum Tone {
        Neutral,
        Accent,
        Ready,
        Caution,
        Danger
    }

    property string text: ""
    property string iconName: ""
    property int tone: Tag.Neutral
    readonly property color ink: tone === Tag.Accent ? KodosiTheme.accentStrong
        : tone === Tag.Ready ? KodosiTheme.ready
        : tone === Tag.Caution ? KodosiTheme.caution
        : tone === Tag.Danger ? KodosiTheme.danger
        : KodosiTheme.inkMuted

    implicitWidth: row.implicitWidth + 16
    implicitHeight: 20
    radius: 10
    color: tone === Tag.Accent ? KodosiTheme.accentSoft
        : tone === Tag.Ready ? KodosiTheme.readySoft
        : tone === Tag.Caution ? KodosiTheme.cautionSoft
        : tone === Tag.Danger ? KodosiTheme.dangerSoft
        : KodosiTheme.alpha(KodosiTheme.ink, 0.08)

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: 4

        KIcon {
            visible: root.iconName.length > 0
            Layout.preferredWidth: 10
            Layout.preferredHeight: 10
            name: root.iconName
            color: root.ink
            strokeWidth: 2.2
        }
        Text {
            text: root.text
            textFormat: Text.PlainText
            color: root.ink
            font.pixelSize: KodosiTheme.fontCaption
            font.weight: Font.Medium
        }
    }
}
