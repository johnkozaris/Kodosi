pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Shapes
import QtQuick.Templates as T
import Kodosi.Models 1.0 as Models

Item {
    id: root

    required property string sessionId
    required property var browser
    property string askTitle: ""
    property real pressedAt: 0
    property point pressedPoint: Qt.point(0, 0)
    property int pressedButton: Qt.NoButton
    property int clicks: 0
    readonly property string browserState: browser.state || ""
    readonly property var activeTab: (browser.tabs || []).find(tab => tab.active) || null
    readonly property string failure: browser.problem || ""
    readonly property var parts: video.window
    readonly property var tabs: (parts.tabs || []).length > 0 ? parts.tabs
        : (browser.tabs || []).map(tab => ({ id: tab.id, title: tab.title, address: tab.url, active: tab.active, loading: false }))
    readonly property bool choosing: !!parts.chooser

    signal asking(string words)

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

    function open(): void {
        const typed = address.text.trim();
        if (typed.length === 0)
            return;
        video.step({ kind: "open", address: typed });
        surface.forceActiveFocus();
    }
    function newTab(): void {
        video.step({ kind: "newTab" });
        address.text = "";
        address.forceActiveFocus();
    }
    function closeTab(id: string): void {
        video.step({ kind: "closeTab", id: id });
    }
    function tabTitle(tab: var): string {
        if (tab.address.length === 0 || tab.address === "about:blank")
            return qsTr("New tab");
        if (tab.title.length > 0 && tab.title !== tab.address)
            return tab.title;
        try {
            return new URL(tab.address).hostname || tab.address;
        } catch (error) {
            return tab.address;
        }
    }
    function cursor(shape: string): int {
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
    function press(mouse: MouseEvent): void {
        const now = Date.now();
        const near = Math.abs(mouse.x - pressedPoint.x) < 5 && Math.abs(mouse.y - pressedPoint.y) < 5;
        clicks = now - pressedAt < 450 && near ? Math.min(3, clicks + 1) : 1;
        pressedAt = now;
        pressedPoint = Qt.point(mouse.x, mouse.y);
        pressedButton = mouse.button;
        video.pointer("press", mouse.x, mouse.y, mouse.button, clicks, mouse.modifiers);
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            id: strip

            Layout.fillWidth: true
            Layout.preferredHeight: 32
            visible: root.browserState === "awake"

            Flickable {
                id: tabsArea

                x: 6
                width: Math.min(contentWidth, strip.width - x - newTab.width - 10)
                height: 26
                anchors.verticalCenter: parent.verticalCenter
                contentWidth: tabRow.implicitWidth
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                WheelHandler {
                    onWheel: event => {
                        const step = event.pixelDelta.x || event.pixelDelta.y || (event.angleDelta.x || event.angleDelta.y) / 2;
                        tabsArea.contentX = Math.max(0, Math.min(tabsArea.contentWidth - tabsArea.width, tabsArea.contentX - step));
                    }
                }
                Row {
                    id: tabRow

                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    objectName: "canvas.browser.tabs"
                    Accessible.id: objectName
                    Accessible.role: Accessible.PageTabList
                    Accessible.name: qsTr("Tabs")

                    add: Transition {
                        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: KodosiTheme.motionFade; easing.type: Easing.OutCubic }
                        NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic }
                    }

                    Repeater {
                        id: tabList

                        model: Models.KeyedList { values: root.tabs }

                        AbstractButton {
                            id: tab

                            required property var modelData
                            required property int index
                            readonly property bool selected: modelData.active
                            readonly property string title: root.tabTitle(modelData)
                            readonly property string favicon: video.icons[modelData.id] || ""

                            function keepInView(): void {
                                if (selected || activeFocus || close.activeFocus)
                                    Qt.callLater(() => {
                                        tabRow.forceLayout();
                                        tabsArea.positionViewAtChild(tab, Flickable.Contain);
                                    });
                            }

                            height: 24
                            implicitWidth: Math.max(80, Math.min(200, tabContent.implicitWidth + 34))
                            hoverEnabled: true
                            objectName: "canvas.browser.tab." + modelData.id
                            Accessible.id: objectName
                            Accessible.name: title
                            Accessible.role: Accessible.PageTab
                            Accessible.selected: selected
                            ToolTip.text: modelData.address
                            ToolTip.visible: hovered && !close.hovered && ToolTip.text.length > 0
                            ToolTip.delay: Tip.delay
                            Keys.onReturnPressed: clicked()
                            Keys.onEnterPressed: clicked()

                            contentItem: Item {
                                RowLayout {
                                    id: tabContent

                                    x: 8
                                    width: tab.width - 34
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 6

                                    Item {
                                        Layout.preferredWidth: 14
                                        Layout.preferredHeight: 14

                                        Spinner {
                                            anchors.centerIn: parent
                                            size: 10
                                            visible: tab.modelData.loading
                                        }
                                        Image {
                                            anchors.fill: parent
                                            visible: !tab.modelData.loading && tab.favicon.length > 0
                                            source: tab.favicon
                                            sourceSize: Qt.size(28, 28)
                                            smooth: true
                                        }
                                        KIcon {
                                            anchors.centerIn: parent
                                            width: 12
                                            height: 12
                                            visible: !tab.modelData.loading && tab.favicon.length === 0
                                            name: "globe"
                                            strokeWidth: 2.2
                                            color: tab.selected ? KodosiTheme.accentStrong : KodosiTheme.inkFaint
                                        }
                                    }
                                    PlainLabel {
                                        Layout.fillWidth: true
                                        text: tab.title
                                        color: tab.selected || tab.hovered ? KodosiTheme.ink : KodosiTheme.inkMuted
                                        font.pixelSize: KodosiTheme.fontFootnote
                                        font.weight: Font.Medium
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                            background: Item {
                                Raised {
                                    anchors.fill: parent
                                    radius: height / 2
                                    opacity: tab.selected ? 1 : 0
                                    visible: opacity > 0

                                    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }
                                }
                                Rectangle {
                                    anchors.fill: parent
                                    radius: height / 2
                                    color: "transparent"
                                    border.width: tab.visualFocus ? 1.5 : 0
                                    border.color: KodosiTheme.accentStrong
                                }
                            }
                            onClicked: video.step({ kind: "tab", id: modelData.id })
                            onSelectedChanged: keepInView()
                            onActiveFocusChanged: keepInView()
                            onXChanged: keepInView()
                            onWidthChanged: keepInView()
                            Component.onCompleted: keepInView()
                            T.ContextMenu.onRequested: position => {
                                tabMenu.tabId = modelData.id;
                                tabMenu.popupFor(tab, position);
                            }

                            RemoveKey {
                                id: close

                                z: 2
                                anchors.right: parent.right
                                anchors.rightMargin: 5
                                anchors.verticalCenter: parent.verticalCenter
                                size: 16
                                objectName: "canvas.browser.close." + tab.modelData.id
                                Accessible.name: qsTr("Close tab")
                                onClicked: {
                                    if (visualFocus)
                                        (tabList.itemAt(tab.index + 1) ?? tabList.itemAt(tab.index - 1) ?? newTab).forceActiveFocus(Qt.TabFocusReason);
                                    root.closeTab(tab.modelData.id);
                                }
                                onActiveFocusChanged: tab.keepInView()
                            }
                        }
                    }
                }
            }
            KIconButton {
                id: newTab

                x: tabsArea.x + tabsArea.width + 4
                anchors.verticalCenter: parent.verticalCenter
                glyph: "plus"
                size: 26
                Accessible.name: qsTr("New tab")
                objectName: "canvas.browser.newTab"
                onClicked: root.newTab()
            }
            KMenu {
                id: tabMenu

                property string tabId: ""

                KMenuItem {
                    text: qsTr("Close tab")
                    iconName: "close"
                    onTriggered: root.closeTab(tabMenu.tabId)
                }
            }
        }
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

                readonly property string icon: root.activeTab && !activeFocus ? video.icons[root.activeTab.id] || "" : ""

                Layout.fillWidth: true
                Layout.maximumWidth: 560
                compact: true
                leftPadding: icon.length > 0 ? 30 : 10
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

                Image {
                    anchors.left: parent.left
                    anchors.leftMargin: 9
                    anchors.verticalCenter: parent.verticalCenter
                    width: 16
                    height: 16
                    visible: address.icon.length > 0
                    source: address.icon
                    sourceSize: Qt.size(32, 32)
                    smooth: true
                }
            }
            Tag {
                readonly property string explanation: qsTr("This terminal is shared, so its browser has its own sign-ins, apart from yours. Everyone in the terminal uses them.")

                visible: root.browser.ownSignIns === true
                text: qsTr("Shared")
                iconName: "people"
                objectName: "canvas.browser.shared"
                Accessible.role: Accessible.StaticText
                Accessible.name: text
                Accessible.description: explanation
                ToolTip.visible: sharedHover.hovered
                ToolTip.text: explanation
                ToolTip.delay: Tip.delay

                HoverHandler { id: sharedHover }
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

                AgentMark { kind: Models.DesktopSettings.agentNamed(root.browser.last ? root.browser.last.agent : ""); size: 14 }
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
                canvas: Models.Canvas
                sessionId: root.sessionId
                active: !root.browser.problem && KodosiTheme.shown
                visible: root.browserState === "awake" && shows
            }
            Spinner {
                anchors.centerIn: parent
                size: 18
                visible: root.browserState === "awake" && !video.shows
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
                onCanceled: video.pointer("release", root.pressedPoint.x, root.pressedPoint.y, root.pressedButton, root.clicks, 0)
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
                readonly property string kind: Models.DesktopSettings.agentNamed(shown ? shown.agent : "")
                readonly property color tint: Identity.kindTint(kind)

                function place(): void {
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
                        kind: pointer.kind
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
            ReconnectingNotice {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                anchors.topMargin: 10
                shown: root.browserState === "awake" && video.shows && video.reconnecting
                objectName: "canvas.browser.reconnecting"
            }
            KMenu {
                id: pageMenu

                readonly property var list: root.parts.menu || null
                readonly property var context: list ? list.context || null : null
                readonly property string shown: list ? JSON.stringify(list) : ""

                function choose(choice: var): void {
                    video.step(choice === undefined ? { kind: "choose", menu: list.id } : { kind: "choose", menu: list.id, choice: choice });
                }

                onShownChanged: {
                    if (!list) {
                        close();
                        return;
                    }
                    const at = video.viewPoint(list.x, list.y);
                    popup(video, at ? at.x : video.width / 2, at ? at.y : video.height / 3);
                }
                onClosed: {
                    if (list)
                        choose();
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
                        onTriggered: pageMenu.choose(index)
                    }
                }
                KMenuItem {
                    text: qsTr("Copy")
                    iconName: "copy"
                    visible: !!pageMenu.context
                    enabled: !!pageMenu.context && pageMenu.context.selection.length > 0
                    onTriggered: pageMenu.choose(0)
                }
                KMenuItem {
                    text: qsTr("Copy link")
                    iconName: "link"
                    visible: !!pageMenu.context && !!pageMenu.context.link
                    onTriggered: pageMenu.choose(1)
                }
                KMenuItem {
                    text: qsTr("Paste")
                    iconName: "document"
                    visible: !!pageMenu.context
                    onTriggered: {
                        pageMenu.choose();
                        video.paste();
                    }
                }
                MenuSeparator {
                    visible: ask.visible
                    height: visible ? implicitHeight : 0
                    contentItem: Rectangle { implicitHeight: 1; color: KodosiTheme.hairline }
                }
                KMenuItem {
                    id: ask

                    text: root.askTitle
                    iconName: "chat"
                    visible: !!pageMenu.context && root.askTitle.length > 0
                    onTriggered: {
                        const words = pageMenu.context.selection || pageMenu.context.link || "";
                        pageMenu.choose();
                        root.asking(words);
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
                    : root.browserState === "starting" ? qsTr("Starting the browser") : qsTr("The browser is asleep")
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
