import Kodosi 1.0
import QtQuick

Raised {
    property bool focused: false

    radius: 20
    fill: look.lifted
    elevation: focused ? 2 : 1
}
