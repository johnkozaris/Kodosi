import Kodosi 1.0
import QtQuick
import QtQuick.Controls

KPopover {
    y: parent ? parent.height + 6 : 0
    margins: 10
    radius: KodosiTheme.radiusLg
    fill: KodosiTheme.raised
    transformOrigin: Popup.TopLeft
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    focus: true
}
