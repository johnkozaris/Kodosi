pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Layouts
import Kodosi.Models 1.0 as Models

Item {
    id: root

    objectName: "stage"
    Accessible.id: objectName
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Terminal stage, %1").arg(terminalCountText)
    Accessible.description: overflowAccessibilityDescription

    signal inspectSessionRequested(string sessionId, string sessionName)
    signal shareSessionRequested(string sessionId, string sessionName)
    signal newSessionRequested()
    signal showSidebarRequested()
    property bool sidebarOpen: true
    property bool interactionEnabled: true
    property bool dividerDragging: false
    property string selectedSessionName
    property bool terminalFocusPending: false
    property bool overflowUpdatePending: false
    property bool verticalOverflow: false
    property var terminalTiles: []
    readonly property bool hasStage:
        Models.DesktopState.stagedSessionIds.length > 0
    readonly property bool focusMode:
        Models.DesktopState.stageLayoutMode
            === Models.DesktopState.Focus
    readonly property bool catalogLoading:
        Models.Sessions.authorityState === Models.Sessions.Loading
            && Models.Sessions.count === 0
    readonly property bool catalogFailed:
        Models.Sessions.authorityState === Models.Sessions.Failed
            && Models.Sessions.count === 0
    readonly property int terminalCount:
        Models.DesktopState.stagedSessionIds.length
    readonly property string terminalCountText:
        terminalCount === 1
        ? qsTr("1 terminal")
        : qsTr("%1 terminals").arg(terminalCount)
    readonly property string overflowAccessibilityDescription:
        verticalOverflow ? qsTr("Scroll for more terminals") : ""

    function refreshSelectedSessionName() {
        const presentation = Models.Sessions.presentationForSession(
            Models.DesktopState.selectedSessionId)
        selectedSessionName =
            presentation.sessionId === Models.DesktopState.selectedSessionId
            ? presentation.name
            : ""
    }

    function focusSelectedTerminal() {
        if (!interactionEnabled || !stageFlick.visible)
            return
        const selected = Models.DesktopState.selectedSessionId
        for (let index = 0; index < terminalTiles.length; ++index) {
            const tile = terminalTiles[index]
            if (tile && tile.visible && tile.sessionId === selected) {
                tile.forceTerminalFocus()
                return
            }
        }
    }

    function registerTerminalTile(tile) {
        if (!tile || terminalTiles.indexOf(tile) >= 0)
            return
        const next = terminalTiles.slice()
        next.push(tile)
        terminalTiles = next
        scheduleTerminalFocus()
    }

    function unregisterTerminalTile(tile) {
        const index = terminalTiles.indexOf(tile)
        if (index < 0)
            return
        const next = terminalTiles.slice()
        next.splice(index, 1)
        terminalTiles = next
        scheduleTerminalFocus()
    }

    function scheduleTerminalFocus() {
        if (!interactionEnabled || terminalFocusPending)
            return
        terminalFocusPending = true
        Qt.callLater(function() {
            terminalFocusPending = false
            focusSelectedTerminal()
        })
    }

    function scheduleVerticalOverflowUpdate() {
        if (overflowUpdatePending)
            return
        overflowUpdatePending = true
        Qt.callLater(function() {
            overflowUpdatePending = false
            verticalOverflow = !focusMode
                && stageFlick.visible
                && stageFlick.contentHeight > stageFlick.height
        })
    }

    onFocusModeChanged: {
        if (focusMode) {
            stageFlick.contentY = 0
            verticalOverflow = false
        } else {
            scheduleVerticalOverflowUpdate()
        }
        scheduleTerminalFocus()
    }
    onInteractionEnabledChanged: {
        if (interactionEnabled)
            scheduleTerminalFocus()
    }
    Component.onCompleted: {
        refreshSelectedSessionName()
        scheduleTerminalFocus()
        scheduleVerticalOverflowUpdate()
    }

    Connections {
        target: Models.DesktopState

        function onSelectedSessionIdChanged() {
            root.refreshSelectedSessionName()
            root.scheduleTerminalFocus()
        }
    }

    Connections {
        target: Models.Sessions

        function onModelReset() {
            root.refreshSelectedSessionName()
        }

        function onDataChanged() {
            root.refreshSelectedSessionName()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: KodosiTheme.canvas
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            visible: root.focusMode && root.hasStage
            Layout.fillWidth: true
            Layout.preferredHeight: visible ? 34 : 0
            color: KodosiTheme.surfaceRaised

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: KodosiTheme.spacing4
                anchors.rightMargin: KodosiTheme.spacing4
                spacing: KodosiTheme.spacing3

                KButton {
                    objectName: "stage.focus.exit"
                    Accessible.id: objectName
                    Accessible.ignored: !visible
                    compact: true
                    variant: KButton.Quiet
                    iconName: "chevron-left"
                    text: qsTr("Grid")
                    Accessible.name: qsTr("Return to terminal grid")
                    onClicked: Models.DesktopState.exitFocusMode()
                }

                PlainLabel {
                    objectName: "stage.focus.title"
                    Accessible.id: objectName
                    Accessible.ignored: !visible
                    Layout.fillWidth: true
                    text: root.selectedSessionName.length > 0
                        ? root.selectedSessionName
                        : qsTr("Terminal")
                    Accessible.name: text
                    color: KodosiTheme.textSecondary
                    font.pixelSize: 10
                    font.weight: Font.DemiBold
                }
            }

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: KodosiTheme.seam
            }
        }

        Item {
            id: stageViewport
            Layout.fillWidth: true
            Layout.fillHeight: true

            Binding {
                target: Models.TerminalTiling
                property: "viewportWidth"
                value: stageFlick.width
            }
            Binding {
                target: Models.TerminalTiling
                property: "viewportHeight"
                value: stageViewport.height
            }
            Binding {
                target: Models.TerminalTiling
                property: "stagedSessionIds"
                value: Models.DesktopState.stagedSessionIds
            }
            Binding {
                target: Models.TerminalTiling
                property: "selectedSessionId"
                value: Models.DesktopState.selectedSessionId
            }
            Binding {
                target: Models.TerminalTiling
                property: "layoutMode"
                value: Models.DesktopState.stageLayoutMode
            }
            Binding {
                target: Models.TerminalTiling
                property: "accessibilityTextScale"
                value: Math.max(
                    1,
                    stageFontMetrics.height / 16)
            }

            FontMetrics {
                id: stageFontMetrics
            }

            Flickable {
                id: stageFlick
                objectName: "stage.scroll"
                Accessible.id: objectName
                Accessible.role: Accessible.Pane
                Accessible.name: qsTr("Terminal grid, %1").arg(
                    root.terminalCountText)
                Accessible.description:
                    root.overflowAccessibilityDescription
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.rightMargin: root.verticalOverflow
                    ? stageScrollBar.width
                    : 0
                visible: root.hasStage
                    && !root.catalogLoading
                    && !root.catalogFailed
                clip: true
                contentWidth: width
                contentHeight: Models.TerminalTiling.contentHeight
                boundsBehavior: Flickable.StopAtBounds
                interactive: !root.focusMode
                    && !root.dividerDragging
                onContentHeightChanged:
                    root.scheduleVerticalOverflowUpdate()
                onHeightChanged:
                    root.scheduleVerticalOverflowUpdate()
                onVisibleChanged:
                    root.scheduleVerticalOverflowUpdate()
                onContentYChanged: {
                    if (root.focusMode && contentY !== 0)
                        contentY = 0
                }

                Item {
                    id: stageContent
                    width: stageFlick.width
                    height: stageFlick.contentHeight

                    Repeater {
                        id: stageEntries
                        objectName: "stage.entries"
                        Accessible.id: objectName
                        model: Models.TerminalTiling

                        delegate: Models.AccessibilityScope {
                            id: layoutEntry

                            required property int entryType
                            required property string stableId
                            required property string sessionId
                            required property real layoutX
                            required property real layoutY
                            required property real layoutWidth
                            required property real layoutHeight
                            required property bool entryVisible
                            required property int orientation
                            required property real percentage

                            objectName: "stage.entry." + stableId
                            Accessible.id: objectName
                            x: layoutX
                            y: layoutY
                            width: layoutWidth
                            height: layoutHeight
                            visible: entryVisible
                            suppressed: !root.interactionEnabled || !entryVisible

                            Loader {
                                anchors.fill: parent
                                active: true
                                onLoaded: {
                                    if (layoutEntry.entryType
                                            === Models.TerminalTiling.Tile)
                                        root.registerTerminalTile(item)
                                    root.scheduleTerminalFocus()
                                }
                                Component.onDestruction:
                                    root.unregisterTerminalTile(item)
                                sourceComponent:
                                    layoutEntry.entryType === Models.TerminalTiling.Tile
                                    ? tileComponent
                                    : dividerComponent
                            }

                            Component {
                                id: tileComponent

                                TerminalTile {
                                    sessionId: layoutEntry.sessionId
                                    interactionEnabled: root.interactionEnabled && layoutEntry.entryVisible
                                    accessibilitySuppressed:
                                        !layoutEntry.entryVisible
                                    focusedSizeAuthority:
                                        root.focusMode
                                        && layoutEntry.sessionId
                                            === Models.DesktopState
                                                .selectedSessionId
                                    onInspectSessionRequested:
                                        (sessionId, sessionName) =>
                                            root.inspectSessionRequested(
                                                sessionId,
                                                sessionName)
                                    onShareSessionRequested:
                                        (sessionId, sessionName) =>
                                            root.shareSessionRequested(
                                                sessionId,
                                                sessionName)
                                }
                            }

                            Component {
                                id: dividerComponent

                                TerminalDivider {
                                    enabled: root.interactionEnabled
                                    dividerId: layoutEntry.stableId
                                    orientation: layoutEntry.orientation
                                    percentage: layoutEntry.percentage
                                    coordinateSpace: stageContent
                                    adjustmentHandler: delta =>
                                        Models.TerminalTiling.adjustDivider(
                                            layoutEntry.stableId,
                                            delta)
                                    onDragActiveChanged:
                                        root.dividerDragging = dragActive
                                }
                            }
                        }
                    }
                }

            }

            KScrollBar {
                id: stageScrollBar
                objectName: "stage.scrollbar"
                Accessible.id: objectName
                Accessible.name: qsTr("Terminal grid scroll bar")
                Accessible.ignored: !visible
                anchors.top: stageViewport.top
                anchors.right: stageViewport.right
                anchors.bottom: stageViewport.bottom
                orientation: Qt.Vertical
                prominent: true
                policy: root.verticalOverflow
                    ? KScrollBar.AlwaysOn
                    : KScrollBar.AlwaysOff
                visible: policy === KScrollBar.AlwaysOn
                size: stageFlick.contentHeight > 0
                    ? Math.min(
                        1,
                        stageFlick.height / stageFlick.contentHeight)
                    : 1
                position: stageFlick.contentHeight > 0
                    ? stageFlick.contentY / stageFlick.contentHeight
                    : 0
                active: visible
                onPositionChanged: {
                    if (pressed)
                        stageFlick.contentY =
                            position * stageFlick.contentHeight
                }
            }

            ColumnLayout {
                anchors.centerIn: parent
                width: Math.min(480, parent.width - 48)
                spacing: KodosiTheme.spacing4
                visible: !stageFlick.visible

                KBusyIndicator {
                    objectName: "stage.sessions.loading"
                    Accessible.id: objectName
                    Accessible.name: qsTr("Loading terminals")
                    Accessible.ignored: !visible
                    Layout.alignment: Qt.AlignHCenter
                    visible: root.catalogLoading
                    running: visible
                    implicitWidth: 28
                    implicitHeight: 28
                }

                KIcon {
                    Accessible.ignored: true
                    Layout.alignment: Qt.AlignHCenter
                    visible: !root.catalogLoading
                    Layout.preferredWidth: 30
                    Layout.preferredHeight: 30
                    name: root.catalogFailed ? "terminal" : "grid"
                    color: root.catalogFailed
                        ? KodosiTheme.danger
                        : KodosiTheme.accent
                }

                PlainLabel {
                    Accessible.ignored: !visible
                    Layout.fillWidth: true
                    text: root.catalogFailed
                        ? qsTr("Terminals unavailable")
                        : root.catalogLoading
                          ? qsTr("Loading terminals...")
                          : Models.Sessions.count === 0
                            ? qsTr("No terminals")
                            : qsTr("No terminals open")
                    color: KodosiTheme.textPrimary
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                    horizontalAlignment: Text.AlignHCenter
                }

                PlainLabel {
                    Accessible.ignored: !visible
                    Layout.fillWidth: true
                    text: Models.Sessions.authorityError
                    color: KodosiTheme.textSecondary
                    font.pixelSize: 12
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                    visible: root.catalogFailed
                }

                KButton {
                    objectName: "stage.sessions.error.retry"
                    Accessible.id: objectName
                    Accessible.ignored: !visible
                    Layout.alignment: Qt.AlignHCenter
                    visible: root.catalogFailed
                    variant: KButton.Directional
                    iconName: "refresh"
                    text: qsTr("Retry")
                    Accessible.name: qsTr("Retry loading terminals")
                    onClicked: Models.SessionActions.refresh()
                }

                KButton {
                    objectName: "stage.empty.new"
                    Accessible.id: objectName
                    Accessible.ignored: !visible
                    Layout.alignment: Qt.AlignHCenter
                    visible: !root.catalogFailed
                        && !root.catalogLoading
                        && Models.Sessions.count === 0
                    variant: KButton.Directional
                    iconName: "chevron-right"
                    text: qsTr("New terminal")
                    Accessible.name: text
                    onClicked: root.newSessionRequested()
                }

                KButton {
                    objectName: "stage.empty.showSidebar"
                    Accessible.id: objectName
                    Accessible.ignored: !visible
                    Layout.alignment: Qt.AlignHCenter
                    visible: !root.catalogFailed
                        && !root.catalogLoading
                        && Models.Sessions.count > 0
                        && !root.sidebarOpen
                    text: qsTr("Show Terminals")
                    variant: KButton.Secondary
                    iconName: "sidebar"
                    Accessible.name: qsTr("Show Terminals sidebar")
                    onClicked: root.showSidebarRequested()
                }
            }
        }
    }
}
