pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

KButton {
    id: picker

    property var values: []
    property string current: ""

    signal picked(string value)

    compact: true
    iconName: "chevron-down"
    iconTrailing: true
    text: (values.find(entry => entry.value === current) || values[0] || { label: "" }).label
    Accessible.id: objectName
    onClicked: menu.popup(picker, 0, picker.height + 4)

    KMenu {
        id: menu

        Instantiator {
            model: picker.values
            delegate: KMenuItem {
                required property var modelData
                text: modelData.label
                iconName: modelData.value === picker.current ? "check" : ""
                onTriggered: picker.picked(modelData.value)
            }
            onObjectAdded: (index, object) => menu.insertItem(index, object)
            onObjectRemoved: (index, object) => menu.removeItem(object)
        }
    }
}
