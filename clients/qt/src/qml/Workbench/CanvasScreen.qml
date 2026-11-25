pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The screen of the terminal's computer, which its owner watches and controls from this device.
Item {
    id: root

    required property string sessionId
    required property string computer
    property real pressedAt: 0
    property point pressedPoint: Qt.point(0, 0)
    property int pressedButton: Qt.NoButton
    property int clicks: 0
    // Where the local pointer last moved over the view, and whether the view stopped at the source's
    // edge while it rested there.
    property point mouseAt: Qt.point(0, 0)
    property bool panStopped: false
    readonly property bool records: video.access.screen !== false
    readonly property bool controls: video.access.input !== false
    // The edge band of a zoomed view that moves it, and how fast at the very edge, in points a second.
    readonly property real panBand: 36
    readonly property real fastestPan: 900

    function press(mouse: MouseEvent): void {
        const now = Date.now();
        const near = Math.abs(mouse.x - pressedPoint.x) < 5 && Math.abs(mouse.y - pressedPoint.y) < 5;
        clicks = now - pressedAt < 450 && near ? Math.min(3, clicks + 1) : 1;
        pressedAt = now;
        pressedPoint = Qt.point(mouse.x, mouse.y);
        pressedButton = mouse.button;
        video.pointer("press", mouse.x, mouse.y, mouse.button, clicks, mouse.modifiers);
    }
    function displayName(index: int): string {
        return index <= 0 ? qsTr("Main display") : qsTr("Display %1").arg(index + 1);
    }
    function sourceName(): string {
        const source = video.source;
        if (source.kind === "window") {
            const window = video.windows.find(each => each.id === source.id);
            return window ? window.title || window.app : qsTr("Window");
        }
        if (source.kind === "display")
            return displayName(video.displays.findIndex(each => each.id === source.id));
        return qsTr("Screen");
    }
    function heldButton(buttons: int): int {
        return buttons & Qt.LeftButton ? Qt.LeftButton : buttons & Qt.RightButton ? Qt.RightButton
            : buttons & Qt.MiddleButton ? Qt.MiddleButton : Qt.NoButton;
    }
    // How fast the view moves with the pointer where it is, in points a second.
    function edgePan(along: real, length: real): real {
        const speed = depth => fastestPan * Math.min(Math.max(depth / panBand, 0), 1);
        return speed(panBand - (length - along)) - speed(panBand - along);
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 36
            Layout.leftMargin: 6
            Layout.rightMargin: 6
            spacing: 4

            KButton {
                id: sourceKey

                Layout.maximumWidth: 280
                variant: KButton.Ghost
                compact: true
                iconName: video.source.kind === "window" ? "window" : "display"
                text: root.sourceName()
                objectName: "canvas.screen.source"
                Accessible.name: root.sourceName()
                Accessible.description: qsTr("Show another display or window")
                onClicked: sources.popupFor(sourceKey, Qt.point(0, sourceKey.height + 4))
            }
            KIconButton {
                visible: video.zoomed
                glyph: "shrink"
                size: 26
                Accessible.name: qsTr("Show the whole screen")
                objectName: "canvas.screen.whole"
                onClicked: video.showWhole()
            }
            Tag {
                readonly property string explanation: qsTr("To control %1 from here, turn on Kodosi Screen there in System Settings, under Privacy & Security, Accessibility.")
                    .arg(root.computer)

                visible: !root.controls
                text: qsTr("View only")
                tone: Tag.Caution
                objectName: "canvas.screen.viewOnly"
                Accessible.role: Accessible.StaticText
                Accessible.name: text
                Accessible.description: explanation
                ToolTip.visible: viewOnlyHover.hovered
                ToolTip.text: explanation
                ToolTip.delay: Tip.delay

                HoverHandler { id: viewOnlyHover }
            }
            Item { Layout.fillWidth: true }
        }
        Rectangle {
            id: surface

            Layout.fillWidth: true
            Layout.fillHeight: true
            color: video.shows && root.records ? KodosiTheme.darkLook.terminal : KodosiTheme.sheet
            clip: true
            activeFocusOnTab: true
            objectName: "canvas.screen.picture"
            Accessible.id: objectName
            Accessible.role: Accessible.Graphic
            Accessible.name: root.sourceName()

            Keys.onPressed: event => {
                if (event.matches(StandardKey.Paste)) {
                    video.paste();
                    event.accepted = true;
                    return;
                }
                event.accepted = video.key(event.key, event.text, event.modifiers, true);
            }
            Keys.onReleased: event => {
                if (!event.matches(StandardKey.Paste))
                    event.accepted = video.key(event.key, event.text, event.modifiers, false);
            }

            MouseArea {
                id: area

                anchors.fill: parent
                enabled: root.records
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                hoverEnabled: true

                onPressed: mouse => {
                    surface.forceActiveFocus();
                    root.press(mouse);
                }
                onReleased: mouse => video.pointer("release", mouse.x, mouse.y, mouse.button, root.clicks, mouse.modifiers)
                onCanceled: video.pointer("release", root.pressedPoint.x, root.pressedPoint.y, root.pressedButton, root.clicks, 0)
                onPositionChanged: mouse => {
                    root.mouseAt = Qt.point(mouse.x, mouse.y);
                    root.panStopped = false;
                    video.pointer("move", mouse.x, mouse.y, root.heldButton(mouse.buttons), 0, mouse.modifiers);
                }
                // Ctrl and the wheel zoom the view, as in other apps here; the wheel alone scrolls the computer.
                onWheel: wheel => {
                    if (wheel.modifiers & Qt.ControlModifier) {
                        const steps = (wheel.angleDelta.y !== 0 ? wheel.angleDelta.y : wheel.pixelDelta.y) / 120;
                        video.zoom(Math.pow(1.25, steps), wheel.x, wheel.y);
                        return;
                    }
                    video.wheel(wheel.x, wheel.y, wheel.pixelDelta, wheel.angleDelta, wheel.modifiers);
                }
            }
            // Above the input area, which still gets the events, so that its cursor is the one shown.
            Models.ScreenVideo {
                id: video

                anchors.fill: parent
                documents: Models.CanvasDocuments
                canvas: Models.Canvas
                sessionId: root.sessionId
                active: KodosiTheme.shown
                visible: shows && root.records
            }
            PinchHandler {
                target: null
                onScaleChanged: delta => video.zoom(delta, centroid.position.x, centroid.position.y)
            }
            DragHandler {
                target: null
                minimumPointCount: 2
                onTranslationChanged: delta => video.pan(-delta.x, -delta.y)
            }
            // A zoomed view moves while the pointer rests at its edge, as Screen Sharing's does.
            FrameAnimation {
                readonly property real dx: root.edgePan(root.mouseAt.x, surface.width)
                readonly property real dy: root.edgePan(root.mouseAt.y, surface.height)

                running: video.zoomed && (area.containsMouse || area.pressed) && !root.panStopped
                    && surface.width > root.panBand * 3 && surface.height > root.panBand * 3 && (dx !== 0 || dy !== 0)
                onTriggered: {
                    if (!video.pan(dx * frameTime, dy * frameTime)) {
                        root.panStopped = true;
                        return;
                    }
                    video.pointer("move", root.mouseAt.x, root.mouseAt.y, root.heldButton(area.pressedButtons), 0, 0);
                }
            }
            Image {
                readonly property var at: video.computerPointer

                x: at.x ?? 0
                y: at.y ?? 0
                width: at.width ?? 0
                height: at.height ?? 0
                source: at.image ?? ""
                visible: root.records && !!at.image && !(area.containsMouse && root.controls)
                smooth: true
                Accessible.ignored: true
            }
            EmptyState {
                anchors.centerIn: parent
                width: Math.min(420, parent.width - 48)
                visible: !root.records
                title: qsTr("Kodosi Screen cannot record %1").arg(root.computer)
                message: qsTr("On %1, turn on Kodosi Screen in System Settings, under Privacy & Security, Screen & System Audio Recording.")
                    .arg(root.computer)
            }
            EmptyState {
                anchors.centerIn: parent
                width: Math.min(420, parent.width - 48)
                visible: root.records && !video.shows && video.problem.length > 0
                title: qsTr("The screen did not open")
                message: video.problem
                objectName: "canvas.screen.failed"

                KButton { text: qsTr("Try again"); onClicked: video.retry() }
            }
            Column {
                anchors.centerIn: parent
                spacing: 10
                visible: root.records && !video.shows && video.problem.length === 0

                Spinner {
                    anchors.horizontalCenter: parent.horizontalCenter
                    size: 18
                }
                PlainLabel {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Waiting for the screen")
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                }
            }
            ReconnectingNotice {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                anchors.topMargin: 10
                look: KodosiTheme.darkLook
                shown: video.shows && video.reconnecting
                objectName: "canvas.screen.reconnecting"
            }
        }
    }
    KMenu {
        id: sources

        Instantiator {
            model: video.displays

            delegate: KMenuItem {
                required property var modelData
                required property int index

                text: root.displayName(index)
                iconName: "display"
                onTriggered: video.show("display", modelData.id)
            }
            onObjectAdded: (index, item) => sources.insertItem(index, item)
            onObjectRemoved: (index, item) => sources.removeItem(item)
        }
        Instantiator {
            model: video.windows

            delegate: KMenuItem {
                required property var modelData

                text: modelData.title ? modelData.title + " – " + modelData.app : modelData.app
                iconName: "window"
                onTriggered: video.show("window", modelData.id)
            }
            onObjectAdded: (index, item) => sources.insertItem(video.displays.length + index, item)
            onObjectRemoved: (index, item) => sources.removeItem(item)
        }
    }
}
