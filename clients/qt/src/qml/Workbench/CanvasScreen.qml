pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The screen of the terminal's computer, which its owner watches and controls from this device.
Item {
    id: root

    required property string sessionId
    required property string computer
    // Whether the person at the computer shares its screen.
    required property bool shared
    required property bool offline
    property real pressedAt: 0
    property point pressedPoint: Qt.point(0, 0)
    property int pressedButton: Qt.NoButton
    property int clicks: 0
    // Where the local pointer last moved over the view, and whether the view stopped at the source's
    // edge while it rested there.
    property point mouseAt: Qt.point(0, 0)
    property bool panStopped: false
    readonly property bool live: video.showing === "live"
    readonly property bool pictured: live || video.showing === "resting"
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
                visible: video.zoomed && root.pictured
                glyph: "shrink"
                size: 26
                Accessible.name: qsTr("Show the whole screen")
                objectName: "canvas.screen.whole"
                onClicked: video.showWhole()
            }
            Item { Layout.fillWidth: true }
        }
        PlainLabel {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            Layout.bottomMargin: 6
            visible: !root.controls && root.pictured
            text: qsTr("%1 does not allow control. Allow it in Kodosi's settings there.").arg(root.computer)
            color: KodosiTheme.caution
            font.pixelSize: KodosiTheme.fontFootnote
            font.weight: Font.Medium
            wrapMode: Text.Wrap
            objectName: "canvas.screen.uncontrolled"
        }
        Rectangle {
            id: surface

            Layout.fillWidth: true
            Layout.fillHeight: true
            color: root.pictured ? KodosiTheme.darkLook.terminal : KodosiTheme.sheet
            clip: true
            activeFocusOnTab: true
            objectName: "canvas.screen.picture"
            Accessible.id: objectName
            Accessible.role: Accessible.Graphic
            Accessible.name: root.sourceName()

            Behavior on color { ColorAnimation { duration: KodosiTheme.motionFade } }

            // A paste there pastes what this device copied.
            Keys.onPressed: event => {
                if (event.matches(StandardKey.Paste))
                    video.giveClipboard();
                event.accepted = video.key(event.key, event.text, event.modifiers, true);
            }
            Keys.onReleased: event => event.accepted = video.key(event.key, event.text, event.modifiers, false)

            MouseArea {
                id: area

                anchors.fill: parent
                enabled: root.pictured
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
                    video.wheel(wheel.x, wheel.y, wheel.pixelDelta, wheel.angleDelta, wheel.phase, wheel.modifiers);
                }
            }
            // Above the input area, which still gets the events, so that its cursor is the one shown.
            Models.ScreenVideo {
                id: video

                anchors.fill: parent
                documents: Models.CanvasDocuments
                canvas: Models.Canvas
                sessionId: root.sessionId
                shared: root.shared
                offline: root.offline
                active: KodosiTheme.shown
                rest: root.live ? 0 : 1
                opacity: root.pictured ? 1 : 0
                visible: opacity > 0

                Behavior on rest { NumberAnimation { duration: KodosiTheme.motionFade } }
                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
            }
            PinchHandler {
                target: null
                enabled: root.pictured
                onScaleChanged: delta => video.zoom(delta, centroid.position.x, centroid.position.y)
            }
            DragHandler {
                target: null
                enabled: root.pictured
                minimumPointCount: 2
                onTranslationChanged: delta => video.pan(-delta.x, -delta.y)
            }
            // A zoomed view moves while the pointer rests at its edge, as Screen Sharing's does. A view
            // that follows another device's does not, or resting there would take the screen over.
            FrameAnimation {
                readonly property real dx: root.edgePan(root.mouseAt.x, surface.width)
                readonly property real dy: root.edgePan(root.mouseAt.y, surface.height)

                running: root.pictured && video.zoomed && !video.follows && (area.containsMouse || area.pressed) && !root.panStopped
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
                visible: root.pictured && !!at.image && !(area.containsMouse && root.controls)
                smooth: true
                Accessible.ignored: true
            }
            VideoCover {
                anchors.fill: parent
                showing: video.showing
                computer: root.computer
                failure: qsTr("Can't show the screen")
                problem: video.problem
                objectName: "canvas.screen.cover"
                onRetried: video.retry()
            }
            Item {
                id: copiedNote

                property bool shown: false

                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 10
                implicitWidth: copiedRow.implicitWidth + 24
                implicitHeight: KodosiTheme.scaled(30)
                opacity: shown ? 1 : 0
                visible: opacity > 0
                objectName: "canvas.screen.copied"
                Accessible.role: Accessible.StaticText
                Accessible.name: copiedLabel.text

                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                Connections {
                    function onCopied() {
                        copiedNote.shown = true;
                        copiedNote.Accessible.announce(copiedLabel.text);
                        copiedLeave.restart();
                    }

                    target: video
                }
                Timer {
                    id: copiedLeave

                    interval: 1500
                    onTriggered: copiedNote.shown = false
                }
                Raised {
                    anchors.fill: parent
                    radius: height / 2
                    look: KodosiTheme.darkLook
                    fill: KodosiTheme.darkLook.lifted
                    elevation: 2
                }
                RowLayout {
                    id: copiedRow

                    anchors.centerIn: parent
                    spacing: 6

                    KIcon {
                        Layout.preferredWidth: KodosiTheme.scaled(12)
                        Layout.preferredHeight: KodosiTheme.scaled(12)
                        name: "check"
                        color: KodosiTheme.darkLook.ready
                        strokeWidth: 2.2
                    }
                    PlainLabel {
                        id: copiedLabel

                        text: qsTr("Copied")
                        color: KodosiTheme.darkLook.ink
                        font.pixelSize: KodosiTheme.fontFootnote
                    }
                }
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
