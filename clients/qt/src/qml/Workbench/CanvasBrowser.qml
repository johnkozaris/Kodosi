pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The shared browser of a terminal: the bar, and the picture that people click and type in.
Item {
    id: root

    required property string sessionId
    required property var browser
    property var picture: Models.Canvas.picture(sessionId)
    property bool watching: false
    property real pressedAt: 0
    property point pressedPoint: Qt.point(0, 0)
    property int clicks: 0
    readonly property string browserState: browser.state || ""
    readonly property var activeTab: (browser.tabs || []).find(tab => tab.active) || null
    readonly property string failure: picture.failure || browser.problem || ""

    function follow() {
        if (watching && browserState !== "awake") {
            Models.Canvas.unwatch(sessionId);
            watching = false;
        }
        if (browserState === "awake" && !watching) {
            Models.Canvas.watch(sessionId);
            watching = true;
        }
        if (browserState === "asleep" && !browser.problem && surface.width > 0 && surface.height > 0)
            Models.Canvas.wake(sessionId);
    }
    function resized() {
        Models.Canvas.setBrowserView(sessionId, surface.width, surface.height);
        follow();
    }
    function act(action) {
        Models.Canvas.act(sessionId, action);
    }
    function open() {
        const typed = address.text.trim();
        if (typed.length === 0)
            return;
        act({ do: "open", url: typed.indexOf("://") >= 0 || typed.startsWith("about:") ? typed : "https://" + typed });
        surface.forceActiveFocus();
    }
    function press(mouse) {
        const now = Date.now();
        const near = Math.abs(mouse.x - pressedPoint.x) < 5 && Math.abs(mouse.y - pressedPoint.y) < 5;
        clicks = now - pressedAt < 450 && near ? Math.min(3, clicks + 1) : 1;
        pressedAt = now;
        pressedPoint = Qt.point(mouse.x, mouse.y);
        Models.Canvas.pointer(sessionId, "mousePressed", mouse.x, mouse.y, mouse.button, clicks, mouse.modifiers);
    }

    onBrowserStateChanged: follow()
    Component.onCompleted: resized()
    Component.onDestruction: {
        if (watching)
            Models.Canvas.unwatch(sessionId);
    }

    Connections {
        function onPictureChanged(id) {
            if (id === root.sessionId)
                root.picture = Models.Canvas.picture(id);
        }

        target: Models.Canvas
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

            KIconButton { glyph: "chevron-left"; size: 26; Accessible.name: qsTr("Back"); objectName: "canvas.browser.back"; onClicked: root.act({ do: "back" }) }
            KIconButton { glyph: "chevron-right"; size: 26; Accessible.name: qsTr("Forward"); objectName: "canvas.browser.forward"; onClicked: root.act({ do: "forward" }) }
            KIconButton { glyph: "refresh"; size: 26; Accessible.name: qsTr("Reload"); objectName: "canvas.browser.reload"; onClicked: root.act({ do: "reload" }) }
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
                            onTriggered: root.act({ do: "tab", id: modelData.id })
                        }
                    }
                    MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline } }
                    KMenuItem { text: qsTr("New tab"); iconName: "plus"; onTriggered: root.act({ do: "newTab" }) }
                    KMenuItem {
                        text: qsTr("Close tab")
                        iconName: "close"
                        enabled: !!root.activeTab
                        onTriggered: root.act({ do: "closeTab", id: root.activeTab.id })
                    }
                }
            }
            KIconButton {
                visible: !tabsButton.visible
                glyph: "plus"
                size: 26
                Accessible.name: qsTr("New tab")
                objectName: "canvas.browser.newTab"
                onClicked: root.act({ do: "newTab" })
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
            color: root.browserState === "awake" ? KodosiTheme.terminal : KodosiTheme.surface
            activeFocusOnTab: true
            objectName: "canvas.browser.page"
            Accessible.id: objectName
            Accessible.role: Accessible.Graphic
            Accessible.name: root.activeTab ? root.activeTab.title || root.activeTab.url : qsTr("Shared browser")

            onWidthChanged: root.resized()
            onHeightChanged: root.resized()
            Keys.onPressed: event => {
                if (event.matches(StandardKey.Paste)) {
                    Models.Canvas.paste(root.sessionId);
                    event.accepted = true;
                    return;
                }
                event.accepted = Models.Canvas.key(root.sessionId, event.key, event.text, event.modifiers, true);
            }
            Keys.onReleased: event => {
                if (!event.matches(StandardKey.Paste))
                    event.accepted = Models.Canvas.key(root.sessionId, event.key, event.text, event.modifiers, false);
            }

            Image {
                anchors.fill: parent
                visible: root.browserState === "awake" && root.picture.shows
                source: visible ? "image://canvas/browser/" + root.sessionId + "/" + root.picture.sequence : ""
                fillMode: Image.PreserveAspectFit
                cache: false
                smooth: true
            }
            MouseArea {
                anchors.fill: parent
                enabled: root.browserState === "awake"
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                hoverEnabled: true
                cursorShape: Qt.ArrowCursor

                onPressed: mouse => {
                    surface.forceActiveFocus();
                    root.press(mouse);
                }
                onReleased: mouse => Models.Canvas.pointer(root.sessionId, "mouseReleased", mouse.x, mouse.y, mouse.button, root.clicks, mouse.modifiers)
                onPositionChanged: mouse => {
                    const held = mouse.buttons & Qt.LeftButton ? Qt.LeftButton : mouse.buttons & Qt.RightButton ? Qt.RightButton
                        : mouse.buttons & Qt.MiddleButton ? Qt.MiddleButton : Qt.NoButton;
                    Models.Canvas.pointer(root.sessionId, "mouseMoved", mouse.x, mouse.y, held, 0, mouse.modifiers);
                }
                onWheel: wheel => Models.Canvas.wheel(root.sessionId, wheel.x, wheel.y, wheel.pixelDelta, wheel.angleDelta, wheel.modifiers)
            }
            EmptyState {
                anchors.centerIn: parent
                width: Math.min(320, parent.width - 32)
                visible: root.browserState !== "awake"
                title: root.browserState === "starting" ? qsTr("Starting the browser") : qsTr("The browser sleeps")
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
