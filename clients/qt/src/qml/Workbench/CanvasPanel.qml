pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

// The canvas of a terminal, as the second lobe of its body: the keys of its parts in its own
// header, then the part that it shows. The files show as tabs over the file, or all as cards.
Item {
    id: root

    required property string sessionId
    property bool wide: false
    property real clearance: 10
    property var session: Models.Sessions.presentationForSession(sessionId)
    readonly property var canvas: session.canvas || null
    readonly property var files: canvas ? canvas.items || [] : []
    readonly property string currentKey: Models.Canvas.revision >= 0 && canvas ? Models.Canvas.current(sessionId) : ""
    readonly property string section: currentKey === "browser" ? "browser" : currentKey.length > 0 ? "files" : ""
    readonly property bool shelf: section === "files" && Models.Canvas.revision >= 0 && Models.Canvas.showsShelf(sessionId)
    readonly property var current: currentKey === "browser" && canvas && canvas.browser ? { key: "browser", kind: "browser", browser: canvas.browser }
        : files.filter(item => item.key === currentKey).map(item => ({ key: item.key, kind: item.kind, title: item.name,
            missing: item.missing === true, item: item }))[0] || null
    readonly property string loadKey: current && current.item && !current.missing ? current.key + "\n" + current.item.revision : ""
    readonly property bool local: session.kind === "local"
    readonly property bool agent: Models.DesktopSettings.agentOf(session.program || "") !== "shell"
    property string readyKey: ""
    property int readyCount: 0
    property string failure: ""

    signal widen(bool wide)
    signal hide()
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
    function ask(key) {
        const send = words => {
            if (Models.SessionActions.input(root.sessionId, Models.Canvas.question(root.sessionId, key, words)))
                root.asked();
        };
        const page = body.item as CanvasPage;
        if (page && key === root.currentKey)
            page.selection(send);
        else
            send("");
    }
    function size(bytes) {
        return bytes >= 1048576 ? qsTr("%1 MB").arg((bytes / 1048576).toFixed(1))
            : bytes >= 1024 ? qsTr("%1 KB").arg(Math.round(bytes / 1024)) : qsTr("%1 bytes").arg(bytes);
    }
    function openFileMenu(item) {
        fileMenu.item = item;
        fileMenu.popup();
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
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 38
            Layout.leftMargin: root.wide ? 7 : 7 + root.clearance
            Layout.rightMargin: 6
            spacing: 8

            CanvasDock {
                sessionId: root.sessionId
                chosen: root.section
                inPanel: true
                onPressed: section => Models.Canvas.show(root.sessionId, section)
                onCloseRequested: section => {
                    Models.Canvas.close(root.sessionId, section);
                    if (Models.Canvas.sections(root.sessionId).length === 0)
                        root.hide();
                }
                onOpenRequested: section => section === "browser" ? Models.Canvas.openBrowser(root.sessionId)
                    : Models.Canvas.show(root.sessionId, section)
            }
            Item { Layout.fillWidth: true }
            Row {
                spacing: 0
                opacity: hover.hovered ? 1 : 0.28

                Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionHover } }

                KIconButton {
                    visible: !!root.current && root.session.canControl === true && root.agent
                    glyph: "chat"
                    size: 26
                    onTerminal: true
                    Accessible.name: qsTr("Ask %1 about this").arg(Models.DesktopSettings.agentLabel(root.session.program))
                    objectName: "canvas.ask"
                    onClicked: root.ask(root.currentKey)
                }
                KIconButton {
                    glyph: "focus"
                    size: 26
                    onTerminal: true
                    active: root.wide
                    Accessible.name: root.wide ? qsTr("Side by side") : qsTr("More room")
                    objectName: "canvas.wide"
                    onClicked: root.widen(!root.wide)
                }
                KIconButton {
                    glyph: "sidebar"
                    size: 26
                    onTerminal: true
                    Accessible.name: qsTr("Hide canvas")
                    objectName: "canvas.hide"
                    onClicked: root.hide()
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

            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    Layout.leftMargin: 6
                    Layout.rightMargin: 6
                    visible: root.section === "files"
                    spacing: 4

                    AbstractButton {
                        id: all

                        Layout.preferredHeight: 24
                        implicitWidth: allRow.implicitWidth + 18
                        hoverEnabled: true
                        objectName: "canvas.files.all"
                        Accessible.id: objectName
                        Accessible.name: qsTr("All files")
                        Accessible.selected: root.shelf
                        ToolTip.text: Accessible.name
                        ToolTip.visible: hovered
                        ToolTip.delay: Tip.delay
                        contentItem: Item {
                            Row {
                                id: allRow

                                x: 9
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 5

                                KIcon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 12
                                    height: 12
                                    name: "grid"
                                    strokeWidth: 2.1
                                    color: root.shelf ? KodosiTheme.ink : KodosiTheme.inkMuted
                                }
                                PlainLabel {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: String(root.files.length)
                                    color: root.shelf ? KodosiTheme.ink : KodosiTheme.inkMuted
                                    font.pixelSize: KodosiTheme.fontFootnote
                                    font.weight: Font.Medium
                                }
                            }
                        }
                        background: Raised { radius: height / 2; visible: root.shelf }
                        onClicked: Models.Canvas.showShelf(root.sessionId, !root.shelf)
                    }
                    Flickable {
                        id: tabsArea

                        function reveal(tab) {
                            if (tab.x < contentX)
                                contentX = tab.x;
                            else if (tab.x + tab.width > contentX + width)
                                contentX = tab.x + tab.width - width;
                        }

                        Layout.fillWidth: true
                        Layout.preferredHeight: 26
                        contentWidth: tabs.implicitWidth
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds

                        WheelHandler {
                            onWheel: event => {
                                const step = event.pixelDelta.x || event.pixelDelta.y || (event.angleDelta.x || event.angleDelta.y) / 2;
                                tabsArea.contentX = Math.max(0, Math.min(tabsArea.contentWidth - tabsArea.width, tabsArea.contentX - step));
                            }
                        }
                        Row {
                            id: tabs

                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 2

                            Repeater {
                                model: root.files

                                AbstractButton {
                                    id: tab

                                    required property var modelData
                                    readonly property bool selected: !root.shelf && modelData.key === root.currentKey
                                    readonly property bool fresh: !selected && Models.Canvas.revision >= 0
                                        && Models.Canvas.isNew(root.sessionId, modelData.key)
                                    readonly property bool closing: hovered

                                    height: 24
                                    implicitWidth: Math.min(200, tabRow.implicitWidth + 17)
                                    hoverEnabled: true
                                    objectName: "canvas.file." + modelData.key
                                    Accessible.id: objectName
                                    Accessible.name: modelData.name
                                    Accessible.role: Accessible.PageTab
                                    Accessible.selected: selected
                                    ToolTip.text: modelData.missing === true ? qsTr("%1 is not there now").arg(modelData.key) : modelData.key
                                    ToolTip.visible: hovered
                                    ToolTip.delay: Tip.delay

                                    contentItem: Item {
                                        RowLayout {
                                            id: tabRow

                                            x: 7
                                            width: Math.min(implicitWidth, tab.width - 17)
                                            anchors.verticalCenter: parent.verticalCenter
                                            spacing: 5

                                            KIcon {
                                                Layout.preferredWidth: 12
                                                Layout.preferredHeight: 12
                                                opacity: tab.closing ? 0 : 1
                                                name: root.glyph(tab.modelData.kind)
                                                strokeWidth: 2.2
                                                color: tab.selected ? KodosiTheme.accentStrong : KodosiTheme.inkFaint
                                            }
                                            PlainLabel {
                                                Layout.fillWidth: true
                                                text: tab.modelData.name
                                                color: tab.modelData.missing === true ? KodosiTheme.inkFaint
                                                    : tab.selected || tab.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                                font.pixelSize: KodosiTheme.fontFootnote
                                                font.weight: Font.Medium
                                                elide: Text.ElideMiddle
                                            }
                                        }
                                    }
                                    background: Item {
                                        Raised { anchors.fill: parent; radius: height / 2; visible: tab.selected }
                                        BreathingDot { x: parent.width - 8; y: 2; size: 6; color: KodosiTheme.glowOrange; visible: tab.fresh }
                                    }
                                    onClicked: Models.Canvas.pick(root.sessionId, modelData.key)
                                    onSelectedChanged: {
                                        if (selected)
                                            Qt.callLater(tabsArea.reveal, tab);
                                    }
                                    Component.onCompleted: {
                                        if (selected)
                                            Qt.callLater(tabsArea.reveal, tab);
                                    }

                                    RemoveKey {
                                        x: 5
                                        z: 2
                                        anchors.verticalCenter: parent.verticalCenter
                                        size: 16
                                        visible: tab.closing
                                        objectName: "canvas.remove." + tab.modelData.key
                                        onClicked: Models.Canvas.remove(root.sessionId, tab.modelData.key)
                                    }
                                    TapHandler {
                                        acceptedButtons: Qt.RightButton
                                        onTapped: root.openFileMenu(tab.modelData)
                                    }
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    visible: root.section === "files"
                    color: KodosiTheme.alpha(KodosiTheme.hairline, 0.6)
                }
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    KScrollView {
                        id: shelfView

                        anchors.fill: parent
                        visible: root.shelf
                        contentWidth: availableWidth
                        objectName: "canvas.shelf"
                        Accessible.id: objectName

                        Flow {
                            x: 22
                            y: 24
                            width: shelfView.availableWidth - 44
                            spacing: 18

                            Repeater {
                                model: root.files.slice().reverse()

                                AbstractButton {
                                    id: card

                                    required property var modelData
                                    readonly property bool fresh: Models.Canvas.revision >= 0 && Models.Canvas.isNew(root.sessionId, modelData.key)

                                    width: 140
                                    height: 164
                                    hoverEnabled: true
                                    objectName: "canvas.card." + modelData.key
                                    Accessible.id: objectName
                                    Accessible.name: modelData.name
                                    contentItem: Column {
                                        spacing: 10

                                        Item {
                                            id: sheet

                                            width: card.width
                                            height: 104
                                            transform: [
                                                Rotation {
                                                    origin.x: sheet.width / 2
                                                    origin.y: sheet.height
                                                    angle: card.hovered ? -1.2 : 0

                                                    Behavior on angle { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                                                },
                                                Translate {
                                                    y: card.hovered ? -2 : 0

                                                    Behavior on y { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }
                                                }
                                            ]

                                            Raised {
                                                anchors.fill: parent
                                                radius: KodosiTheme.radiusMd
                                                fill: KodosiTheme.lifted
                                                elevation: card.hovered ? 2 : 1
                                                visible: card.modelData.missing !== true
                                            }
                                            DashedSlot {
                                                anchors.fill: parent
                                                radius: KodosiTheme.radiusMd
                                                visible: card.modelData.missing === true
                                            }
                                            IconTile {
                                                anchors.centerIn: parent
                                                iconName: root.glyph(card.modelData.kind)
                                                tint: card.modelData.missing === true ? KodosiTheme.inkFaint : "#d9a441"
                                                size: 34
                                            }
                                            BreathingDot { x: parent.width - 4; y: -3; size: 7; color: KodosiTheme.glowOrange; visible: card.fresh }
                                        }
                                        PlainLabel {
                                            width: card.width
                                            text: card.modelData.name
                                            color: card.modelData.missing === true ? KodosiTheme.inkFaint : KodosiTheme.ink
                                            font.pixelSize: KodosiTheme.fontFootnote
                                            font.weight: Font.Medium
                                            horizontalAlignment: Text.AlignHCenter
                                            elide: Text.ElideMiddle
                                        }
                                        PlainLabel {
                                            width: card.width
                                            text: card.modelData.missing === true ? qsTr("Not there now")
                                                : (card.modelData.kind === "picture" ? qsTr("Picture") : card.modelData.kind === "pdf" ? qsTr("PDF")
                                                    : card.modelData.kind === "office" ? qsTr("Office file") : qsTr("Page")) + " · " + root.size(card.modelData.bytes || 0)
                                            color: KodosiTheme.inkFaint
                                            font.pixelSize: KodosiTheme.fontCaption
                                            horizontalAlignment: Text.AlignHCenter
                                            elide: Text.ElideRight
                                        }
                                    }
                                    background: Item {}
                                    onClicked: Models.Canvas.pick(root.sessionId, modelData.key)

                                    RemoveKey {
                                        x: -7
                                        y: -7
                                        z: 2
                                        size: 20
                                        visible: card.hovered
                                        backed: true
                                        objectName: "canvas.card.remove." + card.modelData.key
                                        onClicked: Models.Canvas.remove(root.sessionId, card.modelData.key)
                                    }
                                    TapHandler {
                                        acceptedButtons: Qt.RightButton
                                        onTapped: root.openFileMenu(card.modelData)
                                    }
                                }
                            }
                        }
                    }
                    Loader {
                        id: body

                        anchors.fill: parent
                        visible: !root.shelf
                        sourceComponent: !root.current ? null
                            : root.current.kind === "browser" ? browserView
                            : root.current.missing ? missingView
                            : root.readyKey !== root.current.key ? (root.failure.length > 0 ? failureView : loadingView)
                            : root.current.kind === "picture" ? pictureView
                            : root.current.kind === "office" || !Models.CanvasWeb.available ? fileView
                            : pageView
                    }
                    Spinner {
                        anchors.centerIn: parent
                        size: 18
                        visible: !root.current && Models.Canvas.revision >= 0 && Models.Canvas.opensBrowser(root.sessionId)
                    }
                }
            }
        }
    }
    KMenu {
        id: fileMenu

        property var item: null

        KMenuItem {
            text: qsTr("Open in its app")
            iconName: "open"
            visible: root.local && !!fileMenu.item && fileMenu.item.missing !== true
            onTriggered: Models.DesktopFiles.openPath(fileMenu.item.key)
        }
        KMenuItem {
            text: qsTr("Save a copy")
            iconName: "save"
            visible: !root.local && !!fileMenu.item && fileMenu.item.missing !== true
            onTriggered: Models.Canvas.saveCopy(root.sessionId, fileMenu.item.key)
        }
        KMenuItem {
            text: qsTr("Ask %1 about this").arg(Models.DesktopSettings.agentLabel(root.session.program))
            iconName: "chat"
            visible: root.agent && root.session.canControl === true
            onTriggered: root.ask(fileMenu.item.key)
        }
        KMenuItem {
            text: qsTr("Remove from canvas")
            iconName: "close"
            onTriggered: Models.Canvas.remove(root.sessionId, fileMenu.item.key)
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
