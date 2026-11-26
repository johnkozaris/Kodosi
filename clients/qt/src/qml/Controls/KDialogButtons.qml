pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic

DialogButtonBox {
    id: root

    property bool destructive: false

    padding: 16
    topPadding: 4
    spacing: 8
    alignment: Qt.AlignRight
    background: Item {}
    delegate: KButton {
        variant: DialogButtonBox.buttonRole === DialogButtonBox.DestructiveRole ? KButton.Danger
            : DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole
            ? (root.destructive ? KButton.Danger : KButton.Primary)
            : KButton.Ghost
    }
}
