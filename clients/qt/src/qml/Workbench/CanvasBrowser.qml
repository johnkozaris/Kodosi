pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import Kodosi.Models 1.0 as Models

// The shared browser of a terminal: the bar, and the picture that people click and type in.
Item {
    id: root

    required property string sessionId
    required property var browser
    property real pressedAt: 0
    property point pressedPoint: Qt.point(0, 0)
    property int clicks: 0
    readonly property string browserState: browser.state || ""
    readonly property var activeTab: (browser.tabs || []).find(tab => tab.active) || null
    readonly property string failure: browser.problem || ""
    readonly property var parts: video.window
    readonly property bool choosing: !!parts.chooser

    onChoosingChanged: {
        if (choosing && !Models.DesktopFiles.requestPageFiles(sessionId, parts.chooser.several === true))
            video.step({ kind: "files", paths: [] });
    }

    Connections {
        target: Models.Canvas

        function onFilesGiven(sessionId, paths) {
            if (sessionId === root.sessionId)
                video.step({ kind: "files", paths: paths });
        }
    }

    function open() {
        const typed = address.text.trim();
        if (typed.length === 0)
            return;
        video.step({ kind: "open", address: typed });
        surface.forceActiveFocus();
    }
    function cursor(shape) {
        switch (shape) {
        case "pointer": return Qt.PointingHandCursor;
        case "text": return Qt.IBeamCursor;
        case "crosshair": return Qt.CrossCursor;
        case "grab": return Qt.OpenHandCursor;
        case "grabbing": return Qt.ClosedHandCursor;
        case "not-allowed": return Qt.ForbiddenCursor;
        case "wait": return Qt.WaitCursor;
        case "col-resize":
        case "ew-resize": return Qt.SizeHorCursor;
        case "row-resize":
        case "ns-resize": return Qt.SizeVerCursor;
        default: return Qt.ArrowCursor;
        }
    }
    function press(mouse) {
        const now = Date.now();
        const near = Math.abs(mouse.x - pressedPoint.x) < 5 && Math.abs(mouse.y - pressedPoint.y) < 5;
        clicks = now - pressedAt < 450 && near ? Math.min(3, clicks + 1) : 1;
        pressedAt = now;
        pressedPoint = Qt.point(mouse.x, mouse.y);
        video.pointer("press", mouse.x, mouse.y, mouse.button, clicks, mouse.modifiers);
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
            enabled: root.browserState === "awake"

            KIconButton { glyph: "chevron-left"; size: 26; Accessible.name: qsTr("Back"); objectName: "canvas.browser.back"; onClicked: video.step({ kind: "back" }) }
            KIconButton { glyph: "chevron-right"; size: 26; Accessible.name: qsTr("Forward"); objectName: "canvas.browser.forward"; onClicked: video.step({ kind: "forward" }) }
            KIconButton { glyph: "refresh"; size: 26; Accessible.name: qsTr("Reload"); objectName: "canvas.browser.reload"; onClicked: video.step({ kind: "reload" }) }
            KTextField {
                id: address

                Layout.fillWidth: true
                Layout.maximumWidth: 560
                compact: true
                placeholderText: qsTr("Address")
                objectName: "canvas.browser.address"
                Accessible.id: objectName
                Accessible.name: qsTr("Address")
                text: root.activeTab ? root.activeTab.url : ""
                onTextChanged: {
                    if (!activeFocus)
                        cursorPosition = 0;
                }
                onAccepted: root.open()
                onActiveFocusChanged: {
                    if (!activeFocus)
                        text = Qt.binding(() => root.activeTab ? root.activeTab.url : "");
                }
            }
            KButton {
                id: tabsButton

                visible: (root.browser.tabs || []).length > 1
                compact: true
                variant: KButton.Ghost
                text: qsTr("%1 tabs").arg((root.browser.tabs || []).length)
                objectName: "canvas.browser.tabs"
                onClicked: tabsMenu.popup(tabsButton, 0, tabsButton.height + 4)

                KMenu {
                    id: tabsMenu

                    Instantiator {
                        model: root.browser.tabs || []
                        onObjectAdded: (index, item) => tabsMenu.insertItem(index, item)
                        onObjectRemoved: (index, item) => tabsMenu.removeItem(item)
                        delegate: KMenuItem {
                            required property var modelData

                            text: modelData.title || modelData.url
                            iconName: modelData.active ? "check" : "globe"
                            onTriggered: video.step({ kind: "tab", id: modelData.id })
                        }
                    }
                    MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
                    KMenuItem { text: qsTr("New tab"); iconName: "plus"; onTriggered: video.step({ kind: "newTab" }) }
                    KMenuItem {
                        text: qsTr("Close tab")
                        iconName: "close"
                        enabled: !!root.activeTab
                        onTriggered: video.step({ kind: "closeTab", id: root.activeTab.id })
                    }
                }
            }
            KIconButton {
                visible: !tabsButton.visible
                glyph: "plus"
                size: 26
                Accessible.name: qsTr("New tab")
                objectName: "canvas.browser.newTab"
                onClicked: video.step({ kind: "newTab" })
            }
            PlainLabel {
                Layout.fillWidth: true
                Layout.minimumWidth: 60
                visible: root.failure.length > 0
                text: root.failure
                color: KodosiTheme.caution
                font.pixelSize: KodosiTheme.fontCaption
                elide: Text.ElideRight
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.minimumWidth: 80
                visible: root.failure.length === 0 && !!root.browser.last
                spacing: 5

                AgentMark { program: root.browser.last ? root.browser.last.agent : ""; size: 14 }
                PlainLabel {
                    Layout.fillWidth: true
                    text: root.browser.last ? root.browser.last.agent + " · " + root.browser.last.words : ""
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontCaption
                    elide: Text.ElideMiddle
                }
            }
        }
        Rectangle {
            id: surface

            Layout.fillWidth: true
            Layout.fillHeight: true
            color: root.browserState === "awake" ? KodosiTheme.terminal : KodosiTheme.sheet
            activeFocusOnTab: true
            objectName: "canvas.browser.page"
            Accessible.id: objectName
            Accessible.role: Accessible.Graphic
            Accessible.name: root.activeTab ? root.activeTab.title || root.activeTab.url : qsTr("Shared browser")

            Keys.onPressed: event => {
                pointer.shown = null;
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

            Models.BrowserVideo {
                id: video

                anchors.fill: parent
                documents: Models.CanvasDocuments
                sessionId: root.sessionId
                active: !root.browser.problem
                visible: root.browserState === "awake" && shows
            }
            MouseArea {
                anchors.fill: parent
                enabled: root.browserState === "awake"
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                hoverEnabled: true
                cursorShape: root.cursor(root.parts.shape || "")

                onPressed: mouse => {
                    surface.forceActiveFocus();
                    pointer.shown = null;
                    root.press(mouse);
                }
                onReleased: mouse => video.pointer("release", mouse.x, mouse.y, mouse.button, root.clicks, mouse.modifiers)
                onPositionChanged: mouse => {
                    const held = mouse.buttons & Qt.LeftButton ? Qt.LeftButton : mouse.buttons & Qt.RightButton ? Qt.RightButton
                        : mouse.buttons & Qt.MiddleButton ? Qt.MiddleButton : Qt.NoButton;
                    video.pointer("move", mouse.x, mouse.y, held, 0, mouse.modifiers);
                }
                onWheel: wheel => {
                    pointer.shown = null;
                    video.wheel(wheel.x, wheel.y, wheel.pixelDelta, wheel.angleDelta, wheel.modifiers);
                }
            }
            Item {
                id: pointer

                readonly property var doing: root.browser.doing || null
                readonly property string doingKey: doing ? JSON.stringify(doing) : ""
                readonly property string page: root.activeTab ? root.activeTab.url || "" : ""
                property var shown: null
                property real atX: 0
                property real atY: 0
                readonly property string kind: {
                    const name = (shown ? shown.agent : "").toLowerCase();
                    return ["claude", "codex", "copilot", "cursor", "gemini", "opencode"].find(kind => name.indexOf(kind) >= 0) || "shell";
                }
                readonly property color tint: Identity.kindTint(kind)

                function place() {
                    if (!doing || !doing.point)
                        return;
                    const at = video.viewPoint(doing.point.x, doing.point.y);
                    if (at === undefined || at === null)
                        return;
                    glide.enabled = shown !== null;
                    atX = at.x;
                    atY = at.y;
                    shown = doing;
                    if (doing.kind === "click" && !KodosiTheme.reduceMotion)
                        ring.restart();
                }

                anchors.fill: parent
                enabled: false
                opacity: shown ? 1 : 0
                visible: opacity > 0
                Accessible.ignored: true
                onDoingKeyChanged: doing ? place() : leave.restart()
                onPageChanged: shown = null

                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

                Timer {
                    id: leave

                    interval: 1000
                    onTriggered: {
                        if (!root.browser.doing)
                            pointer.shown = null;
                    }
                }
                Item {
                    x: pointer.atX
                    y: pointer.atY

                    Behavior on x { id: glide; NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }
                    Behavior on y { enabled: glide.enabled; NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutCubic } }

                    Rectangle {
                        id: ringShape

                        x: -17
                        y: -17
                        width: 34
                        height: 34
                        radius: 17
                        color: "transparent"
                        border.width: 2
                        border.color: pointer.tint
                        opacity: 0
                        scale: 0.3

                        SequentialAnimation {
                            id: ring

                            PauseAnimation { duration: 240 }
                            ParallelAnimation {
                                NumberAnimation { target: ringShape; property: "scale"; from: 0.3; to: 1.25; duration: 420; easing.type: Easing.OutCubic }
                                SequentialAnimation {
                                    NumberAnimation { target: ringShape; property: "opacity"; from: 0; to: 0.9; duration: 40 }
                                    NumberAnimation { target: ringShape; property: "opacity"; to: 0; duration: 380; easing.type: Easing.OutCubic }
                                }
                            }
                        }
                    }
                    Shape {
                        width: 15
                        height: 21
                        scale: pointer.shown && root.browser.doing && root.browser.doing.kind === "click" ? 0.86 : 1
                        transformOrigin: Item.TopLeft
                        preferredRendererType: Shape.CurveRenderer

                        ShapePath {
                            fillColor: pointer.tint
                            strokeColor: "#ffffff"
                            strokeWidth: 1.4
                            joinStyle: ShapePath.RoundJoin

                            PathSvg { path: "M 0 0 L 0 17 L 4.3 13.2 L 7.4 20 L 10.4 18.7 L 7.4 12.1 L 13 12.1 Z" }
                        }
                    }
                    AgentMark {
                        x: 13
                        y: 17
                        program: pointer.kind
                        size: 18
                        working: root.browser.doing !== undefined && root.browser.doing !== null
                    }
                }
            }
            Repeater {
                model: root.parts.pointers || []

                Rectangle {
                    required property var modelData
                    readonly property var at: video.viewPoint(modelData.x, modelData.y)

                    x: at ? at.x : 0
                    y: at ? at.y : 0
                    visible: !!at
                    width: name.implicitWidth + 10
                    height: name.implicitHeight + 2
                    radius: height / 2
                    color: Identity.personTint(modelData.person)

                    PlainLabel {
                        id: name

                        anchors.centerIn: parent
                        text: parent.modelData.name
                        color: "#ffffff"
                        font.pixelSize: KodosiTheme.fontCaption
                    }
                }
            }
            KMenu {
                id: pageMenu

                readonly property var list: root.parts.menu || null

                onListChanged: list ? popup(video, video.width / 2 - 100, video.height / 3) : close()
                onClosed: {
                    if (list)
                        video.step({ kind: "choose", menu: list.id });
                }

                Instantiator {
                    model: pageMenu.list ? pageMenu.list.items : []
                    onObjectAdded: (index, item) => pageMenu.insertItem(index, item)
                    onObjectRemoved: (index, item) => pageMenu.removeItem(item)
                    delegate: KMenuItem {
                        required property var modelData
                        required property int index

                        text: modelData.label
                        enabled: modelData.enabled
                        iconName: modelData.chosen ? "check" : ""
                        onTriggered: video.step({ kind: "choose", menu: pageMenu.list.id, choice: index })
                    }
                }
            }
            Rectangle {
                id: pageDialog

                readonly property var dialog: root.parts.dialog || null

                anchors.centerIn: parent
                visible: !!dialog
                width: Math.min(340, parent.width - 32)
                height: column.implicitHeight + 28
                radius: KodosiTheme.radiusMd
                color: KodosiTheme.lifted

                ColumnLayout {
                    id: column

                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 10

                    PlainLabel {
                        Layout.fillWidth: true
                        text: pageDialog.dialog ? pageDialog.dialog.message : ""
                        wrapMode: Text.Wrap
                    }
                    KTextField {
                        id: answer

                        Layout.fillWidth: true
                        visible: !!pageDialog.dialog && pageDialog.dialog.kind === "prompt"
                        text: pageDialog.dialog ? pageDialog.dialog.text : ""
                    }
                    RowLayout {
                        Layout.alignment: Qt.AlignRight

                        KButton {
                            visible: !!pageDialog.dialog && pageDialog.dialog.kind !== "alert"
                            text: qsTr("Cancel")
                            onClicked: video.step({ kind: "dialog", accept: false })
                        }
                        KButton {
                            text: qsTr("OK")
                            variant: KButton.Primary
                            onClicked: video.step(answer.visible ? { kind: "dialog", accept: true, text: answer.text }
                                : { kind: "dialog", accept: true })
                        }
                    }
                }
            }
            EmptyState {
                anchors.centerIn: parent
                width: Math.min(320, parent.width - 32)
                visible: root.browserState !== "awake"
                title: root.browser.getting !== undefined ? qsTr("Getting the browser, %1%").arg(root.browser.getting)
                    : root.browserState === "starting" ? qsTr("Starting the browser") : qsTr("The browser sleeps")
                message: root.browserState === "asleep" ? root.browser.problem || "" : ""
                art: KIcon {
                    width: 28
                    height: 28
                    name: root.browserState === "starting" ? "globe" : "moon"
                    color: KodosiTheme.inkMuted
                }

                KButton {
                    visible: root.browserState === "asleep"
                    text: qsTr("Wake it")
                    objectName: "canvas.browser.wake"
                    Accessible.id: objectName
                    onClicked: Models.Canvas.wake(root.sessionId)
                }
            }
        }
    }
}
