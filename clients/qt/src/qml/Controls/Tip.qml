pragma Singleton

import QtQuick

QtObject {
    property bool warm: false
    readonly property int delay: warm ? 0 : 450
}
