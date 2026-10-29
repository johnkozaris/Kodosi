pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

// A terminal with its canvas open: two lobes of one body with a soft waist between them, or
// the canvas over all the stage with the terminal live and zoomed out in its corner. The
// terminal keeps its size in both. With `always`, the body is there without a canvas too, and
// `tab` is the tab that the terminal flows out of.
Item {
    id: root

    default property alias contents: holder.data
    property string canvasSessionId: ""
    property bool wide: false
    property real chosenWidth: 0
    property bool always: false
    property rect tab: Qt.rect(0, 0, 0, 0)
    property real radius: KodosiTheme.radiusTile
    readonly property real narrowest: 380
    readonly property bool open: canvasSessionId.length > 0
    readonly property bool narrow: width < narrowest * 2
    readonly property bool beside: open && !narrow
    readonly property bool zoomed: open && (wide || narrow)
    readonly property bool joined: open || always
    readonly property real panel: Math.min(Math.max(chosenWidth > 0 ? chosenWidth : width / 2, narrowest),
        Math.max(narrowest, width - narrowest))
    readonly property real tileWidth: beside ? width - panel : width
    readonly property real zoom: tileWidth > 0 && height > 0
        ? Math.min(0.45, 320 / tileWidth, width * 0.28 / tileWidth, 0.4) : 1
    readonly property real inset: 16
    property real part: zoomed ? 1 : 0

    signal hideRequested()
    signal wideChosen(bool wide)
    signal widthChosen(real width)
    signal asked()

    Behavior on part { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }

    CanvasBody {
        anchors.fill: parent
        visible: root.joined
        seam: root.zoomed ? 0 : root.beside ? root.width - root.panel : root.width
        depth: root.beside && !root.zoomed ? 14 : 0
        radius: root.radius
        tab: root.tab

        Behavior on seam {
            enabled: root.open
            NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic }
        }
        Behavior on depth { NumberAnimation { duration: KodosiTheme.motionSoft; easing.type: Easing.OutCubic } }
    }
    Loader {
        x: root.zoomed ? 0 : root.width - root.panel
        width: root.zoomed ? root.width : root.panel
        height: root.height
        z: 1
        active: root.open
        opacity: status === Loader.Ready ? 1 : 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

        sourceComponent: CanvasPanel {
            sessionId: root.canvasSessionId
            wide: root.zoomed
            onWiden: wide => root.wideChosen(wide)
            onHide: root.hideRequested()
            onAsked: {
                root.wideChosen(false);
                root.asked();
            }
        }
    }
    Item {
        id: lift

        readonly property real amount: (1 - (1 - root.zoom) * root.part) * (back.containsMouse && root.part > 0.999 ? 1.035 : 1)

        width: root.tileWidth
        height: root.height
        z: 2
        transform: [
            Scale {
                origin.x: lift.width
                origin.y: lift.height
                xScale: lift.amount
                yScale: lift.amount
            },
            Translate {
                x: (root.width - root.inset - root.tileWidth) * root.part
                y: -root.inset * root.part
            }
        ]

        CanvasBody {
            anchors.fill: parent
            visible: root.part > 0.001
            seam: width
            radius: root.radius
        }
        Item {
            id: holder

            anchors.fill: parent
        }
        MouseArea {
            id: back

            anchors.fill: parent
            visible: root.part > 0.001
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            objectName: "canvas.terminal"
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Side by side")
            Accessible.description: qsTr("Shows the terminal beside its canvas")
            onClicked: root.wideChosen(false)
        }
    }
    MouseArea {
        id: handle

        property real startX: 0
        property real startPanel: 0

        x: root.width - root.panel - width / 2
        y: 38
        width: 12
        height: root.height - 38
        z: 3
        visible: root.beside && !root.zoomed
        hoverEnabled: true
        preventStealing: true
        cursorShape: Qt.SizeHorCursor
        Accessible.role: Accessible.Slider
        Accessible.name: qsTr("Resize canvas")

        onPressed: mouse => {
            startX = mapToItem(root, mouse.x, 0).x;
            startPanel = root.panel;
        }
        onPositionChanged: mouse => {
            if (pressed)
                root.widthChosen(Math.max(root.narrowest, Math.min(root.width - root.narrowest,
                    startPanel - (mapToItem(root, mouse.x, 0).x - startX))));
        }
        onDoubleClicked: root.wideChosen(true)

        Rectangle {
            anchors.centerIn: parent
            width: 3
            height: 34
            radius: 1.5
            color: KodosiTheme.terminalInkFaint
            opacity: handle.containsMouse || handle.pressed ? 0.7 : 0

            Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }
        }
    }
}
