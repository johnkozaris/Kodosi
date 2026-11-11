pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls

KMenu {
    id: root

    property var options: []
    property string current: ""
    property string note: ""

    signal picked(string id)

    minimumWidth: 200

    Instantiator {
        model: root.options
        onObjectAdded: (index, item) => root.insertItem(index, item)
        onObjectRemoved: (index, item) => root.removeItem(item)
        delegate: KMenuItem {
            required property var modelData

            text: modelData.label
            hint: modelData.detail || ""
            checkable: true
            checked: modelData.id === root.current
            enabled: modelData.enabled !== false
            objectName: root.objectName + "." + modelData.id
            onTriggered: {
                if (modelData.id !== root.current)
                    root.picked(modelData.id)
            }
        }
    }
    PlainLabel {
        visible: root.note.length > 0
        width: root.availableWidth
        height: visible ? implicitHeight : 0
        leftPadding: 8
        rightPadding: 8
        topPadding: 6
        bottomPadding: 4
        text: root.note
        wrapMode: Text.WordWrap
        color: KodosiTheme.inkMuted
        font.pixelSize: KodosiTheme.fontFootnote
    }
}
