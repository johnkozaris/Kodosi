pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The canvas of a terminal: the items that its agent shows and its shared browser, next to
// the terminal on the stage or in its own tab in a room.
Item {
    id: root

    required property string sessionId
    property bool roomEmbedded: false
    property var session: Models.Sessions.presentationForSession(sessionId)
    readonly property var canvas: session.canvas || null
    readonly property var entries: {
        if (!canvas)
            return [];
        const items = (canvas.items || []).map(item => ({ key: item.key, title: item.name, glyph: root.glyph(item.kind),
                kind: item.kind, missing: item.missing === true, item: item }));
        return canvas.browser ? items.concat([{ key: "browser", title: qsTr("Browser"), glyph: "globe", kind: "browser",
                missing: false, browser: canvas.browser }]) : items;
    }
    readonly property string currentKey: Models.Canvas.revision >= 0 && canvas ? Models.Canvas.current(sessionId) : ""
    readonly property var current: entries.find(entry => entry.key === currentKey) || null
    readonly property string loadKey: current && current.item && !current.missing ? current.key + "\n" + current.item.revision : ""
    readonly property bool local: session.kind === "local"
    property string readyKey: ""
    property int readyCount: 0
    property string failure: ""

    signal closeRequested()
    signal asked()

    function markSeen() {
        if (currentKey.length > 0)
            Models.Canvas.saw(sessionId, currentKey);
    }
    function glyph(kind) {
        return kind === "picture" ? "picture" : kind === "office" ? "office" : "page";
    }
    function load() {
        failure = "";
        if (!current || !current.item || current.missing)
            return;
        if (readyKey !== current.key)
            readyKey = "";
        if (Models.Canvas.load(sessionId, current.key)) {
            readyKey = current.key;
            ++readyCount;
        }
    }
    function ask() {
        const key = current.key;
        const send = words => {
            if (Models.SessionActions.input(root.sessionId, Models.Canvas.question(root.sessionId, key, words)))
                root.asked();
        };
        const page = body.item as CanvasPage;
        if (page)
            page.selection(send);
        else
            send("");
    }

    objectName: "canvas." + sessionId
    Accessible.id: objectName
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Canvas of %1").arg(session.name || qsTr("Terminal"))

    onLoadKeyChanged: load()
    onCurrentKeyChanged: {
        body.active = false;
        body.active = true;
        Qt.callLater(markSeen);
    }

    Connections {
        function onModelReset() {
            root.session = Models.Sessions.presentationForSession(root.sessionId);
        }

        target: Models.Sessions
    }
    Connections {
        function onChanged() {
            Qt.callLater(root.markSeen);
        }
        function onLoaded(id, key) {
            if (id === root.sessionId && root.current && key === root.current.key) {
                root.readyKey = key;
                ++root.readyCount;
            }
        }
        function onFailed(id, key, message) {
            if (id === root.sessionId && root.current && key === root.current.key)
                root.failure = message;
        }

        target: Models.Canvas
    }
    HoverHandler { id: hover }
    Raised {
        anchors.fill: parent
        radius: KodosiTheme.radiusTile
        fill: KodosiTheme.ground
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 38
            topLeftRadius: KodosiTheme.radiusTile
            topRightRadius: KodosiTheme.radiusTile
            color: KodosiTheme.alpha(KodosiTheme.raised, 0.45)

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 11
                anchors.rightMargin: 6
                spacing: 8

                KIcon { Layout.preferredWidth: 14; Layout.preferredHeight: 14; name: "canvas"; strokeWidth: 2.2; color: KodosiTheme.accentStrong }
                PlainLabel { text: qsTr("Canvas"); font.weight: Font.DemiBold }
                PlainLabel {
                    Layout.maximumWidth: Math.max(60, root.width * 0.2)
                    visible: !root.roomEmbedded
                    text: root.session.name || ""
                    color: KodosiTheme.inkMuted
                    font.pixelSize: KodosiTheme.fontFootnote
                    elide: Text.ElideRight
                }
                Flickable {
                    id: chipsArea

                    function reveal(chip) {
                        if (chip.x < contentX)
                            contentX = chip.x;
                        else if (chip.x + chip.width > contentX + width)
                            contentX = chip.x + chip.width - width;
                    }

                    Layout.fillWidth: true
                    Layout.preferredHeight: 30
                    contentWidth: chips.implicitWidth
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds

                    WheelHandler {
                        onWheel: event => {
                            const step = event.pixelDelta.x || event.pixelDelta.y || (event.angleDelta.x || event.angleDelta.y) / 2;
                            chipsArea.contentX = Math.max(0, Math.min(chipsArea.contentWidth - chipsArea.width, chipsArea.contentX - step));
                        }
                    }

                    Row {
                        id: chips

                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2

                        Repeater {
                            model: root.entries

                            AbstractButton {
                                id: chip

                                required property var modelData
                                readonly property bool selected: modelData.key === root.currentKey
                                readonly property bool fresh: !selected && Models.Canvas.revision >= 0
                                    && Models.Canvas.isNew(root.sessionId, modelData.key)

                                height: 26
                                implicitWidth: Math.min(200, chipRow.implicitWidth + 20)
                                hoverEnabled: true
                                objectName: "canvas.chip." + modelData.key
                                Accessible.id: objectName
                                Accessible.name: modelData.title
                                Accessible.role: Accessible.PageTab
                                Accessible.selected: selected
                                ToolTip.text: modelData.missing ? qsTr("Missing") : modelData.kind === "browser"
                                    ? (modelData.browser.tabs || []).filter(tab => tab.active).map(tab => tab.url).join("") || modelData.title
                                    : modelData.key
                                ToolTip.visible: hovered
                                ToolTip.delay: Tip.delay

                                contentItem: Item {
                                    RowLayout {
                                        id: chipRow

                                        x: 10
                                        width: Math.min(implicitWidth, chip.width - 20)
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 5

                                        KIcon {
                                            Layout.preferredWidth: 12
                                            Layout.preferredHeight: 12
                                            name: chip.modelData.glyph
                                            strokeWidth: 2.2
                                            color: chip.selected ? KodosiTheme.ink : KodosiTheme.inkMuted
                                        }
                                        PlainLabel {
                                            Layout.fillWidth: true
                                            text: chip.modelData.title
                                            color: chip.modelData.missing ? KodosiTheme.inkFaint : chip.selected || chip.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                            font.pixelSize: KodosiTheme.fontFootnote
                                            font.weight: Font.Medium
                                            elide: Text.ElideMiddle
                                        }
                                    }
                                }
                                background: Item {
                                    Raised { anchors.fill: parent; radius: height / 2; visible: chip.selected }
                                    BreathingDot { x: parent.width - 8; y: 2; size: 6; color: KodosiTheme.glowOrange; visible: chip.fresh }
                                }
                                onClicked: Models.Canvas.pick(root.sessionId, modelData.key)
                                onSelectedChanged: {
                                    if (selected)
                                        Qt.callLater(chipsArea.reveal, chip);
                                }
                                onXChanged: {
                                    if (selected)
                                        Qt.callLater(chipsArea.reveal, chip);
                                }
                                Component.onCompleted: {
                                    if (selected)
                                        Qt.callLater(chipsArea.reveal, chip);
                                }

                                TapHandler {
                                    acceptedButtons: Qt.RightButton
                                    onTapped: {
                                        chipMenu.key = chip.modelData.key;
                                        chipMenu.popup();
                                    }
                                }
                            }
                        }
                    }
                }
                Row {
                    spacing: 0
                    opacity: hover.hovered ? 1 : 0.28

                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }

                    KIconButton {
                        visible: !!root.current && root.session.canControl === true
                        glyph: "chat"
                        size: 26
                        Accessible.name: Models.DesktopSettings.agentOf(root.session.program || "") === "shell"
                            ? qsTr("Ask about this")
                            : qsTr("Ask %1 about this").arg(Models.DesktopSettings.agentLabel(root.session.program))
                        objectName: "canvas.ask"
                        onClicked: root.ask()
                    }
                    KIconButton {
                        visible: !!root.current && !!root.current.item && !root.current.missing && root.local
                        glyph: "open"
                        size: 26
                        Accessible.name: qsTr("Open in its app")
                        objectName: "canvas.open"
                        onClicked: Models.DesktopFiles.openPath(root.current.key)
                    }
                    KIconButton {
                        visible: !!root.current && !!root.current.item && !root.current.missing && !root.local
                        glyph: "save"
                        size: 26
                        Accessible.name: qsTr("Save a copy")
                        objectName: "canvas.save"
                        onClicked: Models.Canvas.saveCopy(root.sessionId, root.current.key)
                    }
                    KIconButton {
                        glyph: "close"
                        size: 26
                        Accessible.name: root.roomEmbedded ? qsTr("Show the terminal") : qsTr("Close canvas")
                        objectName: "canvas.close"
                        onClicked: root.closeRequested()
                    }
                }
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 3
            Layout.leftMargin: 5
            Layout.rightMargin: 5
            Layout.bottomMargin: 5
            radius: KodosiTheme.radiusSm
            color: KodosiTheme.surface
            clip: true

            Loader {
                id: body

                anchors.fill: parent
                sourceComponent: !root.current ? null
                    : root.current.kind === "browser" ? browserView
                    : root.current.missing ? missingView
                    : root.readyKey !== root.current.key ? (root.failure.length > 0 ? failureView : loadingView)
                    : root.current.kind === "picture" ? pictureView
                    : root.current.kind === "office" || !Models.CanvasWeb.available ? fileView
                    : pageView
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: KodosiTheme.radiusTile
        color: "transparent"
        border.width: 1
        border.color: KodosiTheme.alpha(KodosiTheme.hairline, 0.7)
    }
    KMenu {
        id: chipMenu

        property string key: ""

        KMenuItem {
            text: qsTr("Remove from canvas")
            iconName: "trash"
            destructive: true
            onTriggered: Models.Canvas.remove(root.sessionId, chipMenu.key)
        }
    }
    Component {
        id: browserView

        CanvasBrowser {
            sessionId: root.sessionId
            browser: root.current.browser
        }
    }
    Component {
        id: pageView

        CanvasPage {
            otherComputer: !root.local
            address: root.readyCount >= 0 && root.current ? Models.Canvas.pageAddress(root.sessionId, root.current.key) : ""
        }
    }
    Component {
        id: pictureView

        Item {
            Image {
                anchors.centerIn: parent
                width: Math.min(implicitWidth, parent.width)
                height: Math.min(implicitHeight, parent.height)
                source: root.readyCount >= 0 && root.current ? Models.Canvas.pictureAddress(root.sessionId, root.current.key) : ""
                sourceSize: Qt.size(parent.width * Screen.devicePixelRatio, parent.height * Screen.devicePixelRatio)
                fillMode: Image.PreserveAspectFit
                cache: false
                smooth: true
                Accessible.role: Accessible.Graphic
                Accessible.name: root.current ? root.current.title : ""
            }
        }
    }
    Component {
        id: fileView

        CanvasFile {
            sessionId: root.sessionId
            item: root.current.item
            local: root.local
        }
    }
    Component {
        id: missingView

        Item {
            EmptyState {
                anchors.centerIn: parent
                width: Math.min(340, parent.width - 32)
                title: qsTr("The file is missing")
                message: root.current ? root.current.key : ""
            }
        }
    }
    Component {
        id: failureView

        Item {
            EmptyState {
                anchors.centerIn: parent
                width: Math.min(340, parent.width - 32)
                title: qsTr("This did not load")
                message: root.failure

                KButton { text: qsTr("Try again"); onClicked: root.load() }
            }
        }
    }
    Component {
        id: loadingView

        Item {
            Spinner { anchors.centerIn: parent; size: 18 }
        }
    }
}
