import QtQuick

Item {
    id: root

    property string sessionId: ""
    readonly property bool dragging: handler.active

    Drag.dragType: Drag.Automatic
    Drag.supportedActions: Qt.MoveAction
    Drag.mimeData: ({ "application/x-kodosi-terminal": root.sessionId })
    Drag.active: handler.active

    DragHandler {
        id: handler
        target: null
    }
}
