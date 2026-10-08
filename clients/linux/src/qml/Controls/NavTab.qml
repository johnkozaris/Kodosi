import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Button {
    id: root

    required property string title
    required property string accessibleId
    property string iconName: "command"
    property bool selected: false

    objectName: "header.tab." + accessibleId
    Accessible.id: objectName
    Accessible.name: title
    Accessible.selected: selected

    implicitHeight: 38
    leftPadding: 12
    rightPadding: 12
    topPadding: 4
    bottomPadding: 4

    contentItem: RowLayout {
        spacing: KodosiTheme.spacing2

        KIcon {
            Layout.preferredWidth: 16
            Layout.preferredHeight: 16
            name: root.iconName
            color: root.selected
                ? KodosiTheme.accent
                : KodosiTheme.textSecondary
        }

        Label {
            text: root.title
            color: root.selected
                ? KodosiTheme.textPrimary
                : KodosiTheme.textSecondary
            font.pixelSize: 14
            font.weight: Font.DemiBold
        }
    }

    KFocusIndicator { active: root.activeFocus && root.enabled }

    background: Rectangle {
        radius: KodosiTheme.radiusSmall
        color: root.selected
            ? KodosiTheme.surfaceSelected
            : root.hovered ? KodosiTheme.surfaceElevated : KodosiTheme.surface

        Behavior on color {
            ColorAnimation { duration: KodosiTheme.motionFast }
        }
    }
}
