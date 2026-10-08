import Kodosi 1.0
import QtQuick

Item {
    id: root

    required property string dividerId
    required property int orientation
    required property real percentage
    required property Item coordinateSpace
    required property var adjustmentHandler

    objectName: "stage.divider." + dividerId
    Accessible.id: objectName
    Accessible.role: Accessible.Slider
    Accessible.name: orientation === Qt.Horizontal
        ? qsTr("Resize terminal rows")
        : qsTr("Resize terminal columns")
    Accessible.description: qsTr("%1 percent").arg(
        Math.round(percentage))
    Accessible.focusable: true
    activeFocusOnTab: visible

    readonly property real value: percentage
    readonly property real minimumValue: 0
    readonly property real maximumValue: 100
    readonly property real stepSize: 1
    readonly property bool dragActive: dragArea.pressed

    function applyAdjustment(deltaPixels) {
        const accepted = adjustmentHandler(deltaPixels)
        return Number.isFinite(accepted) ? accepted : 0
    }

    Accessible.onIncreaseAction: root.applyAdjustment(24)
    Accessible.onDecreaseAction: root.applyAdjustment(-24)

    Keys.onLeftPressed: event => {
        if (orientation === Qt.Vertical) {
            root.applyAdjustment(-24)
            event.accepted = true
        }
    }
    Keys.onRightPressed: event => {
        if (orientation === Qt.Vertical) {
            root.applyAdjustment(24)
            event.accepted = true
        }
    }
    Keys.onUpPressed: event => {
        if (orientation === Qt.Horizontal) {
            root.applyAdjustment(-24)
            event.accepted = true
        }
    }
    Keys.onDownPressed: event => {
        if (orientation === Qt.Horizontal) {
            root.applyAdjustment(24)
            event.accepted = true
        }
    }

    Rectangle {
        anchors.fill: parent
        color: root.activeFocus || dragArea.pressed
            ? KodosiTheme.accent
            : dragArea.containsMouse
              ? KodosiTheme.seamStrong
              : KodosiTheme.seam
        opacity: root.activeFocus || dragArea.pressed ? 0.8 : 0.45
    }

    MouseArea {
        id: dragArea
        anchors.fill: parent
        hoverEnabled: true
        preventStealing: true
        cursorShape: root.orientation === Qt.Horizontal
            ? Qt.SizeVerCursor
            : Qt.SizeHorCursor
        property real pressCoordinate: 0
        property real appliedDelta: 0

        function stableCoordinate(mouse) {
            const point = mapToItem(
                root.coordinateSpace,
                mouse.x,
                mouse.y)
            return root.orientation === Qt.Horizontal
                ? point.y : point.x
        }

        onPressed: mouse => {
            root.forceActiveFocus()
            pressCoordinate = stableCoordinate(mouse)
            appliedDelta = 0
        }
        onPositionChanged: mouse => {
            if (!pressed)
                return
            const cumulative =
                stableCoordinate(mouse) - pressCoordinate
            const incremental = cumulative - appliedDelta
            if (incremental !== 0) {
                appliedDelta += root.applyAdjustment(incremental)
            }
        }
    }
}
