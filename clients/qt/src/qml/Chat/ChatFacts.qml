pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

Item {
    id: root

    required property var facts
    required property var session
    required property string sessionId
    required property Look look
    readonly property string agent: session.program || ""
    readonly property var launcher: actions.launcher
    readonly property var models: launcher ? launcher.models[agent] || null : null
    readonly property string model: facts.model || ""
    readonly property string modelName: Models.Chats.modelName(model, models || ({}))
    readonly property var levels: {
        const offer = models ? models.models || [] : []
        const allowed = (offer.find(option => option.id === model) || {}).efforts || []
        return (models ? models.efforts || [] : []).filter(option => allowed.length === 0 || allowed.includes(option.id))
    }
    readonly property int level: levels.findIndex(option => option.id === facts.effort)
    readonly property bool full: row.implicitWidth + (modeWords.visible ? 0 : modeWords.implicitWidth + 4) <= width
    property double now: Date.now()

    function loadModels(): void {
        if (launcher && agent.length > 0 && models === null)
            launcher.loadModels(agent)
    }
    function openModel(): void {
        if (modelKey.visible && modelKey.opens)
            actions.open()
    }
    function openUsage(): void {
        if (usageKey.visible)
            usage.open()
    }

    implicitHeight: 26
    objectName: "chat.facts"

    Component.onCompleted: {
        if (launcher)
            launcher.refresh(false)
        loadModels()
    }
    onAgentChanged: loadModels()
    onLauncherChanged: loadModels()

    Connections {
        function onAgentsChanged() {
            root.loadModels()
        }

        target: root.launcher
    }

    component Fact: Item {
        id: fact

        property string tip: ""
        property Look look: KodosiTheme.look
        default property alias marks: markRow.data

        implicitWidth: markRow.implicitWidth
        implicitHeight: 26
        Accessible.role: Accessible.StaticText
        Accessible.name: tip
        ToolTip.visible: hover.hovered
        ToolTip.text: tip
        ToolTip.delay: Tip.delay

        HoverHandler { id: hover }
        Row {
            id: markRow

            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
        }
    }
    component UsageLine: Row {
        id: usageLine

        property string title: ""
        property string detail: ""
        property Look look: KodosiTheme.look
        default property alias sign: signBox.data

        spacing: 10
        Accessible.role: Accessible.StaticText
        Accessible.name: detail.length > 0 ? title + ", " + detail : title

        Item {
            id: signBox

            anchors.verticalCenter: parent.verticalCenter
            width: 22
            height: 22
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2

            PlainLabel {
                text: usageLine.title
                color: usageLine.look.ink
                font.pixelSize: KodosiTheme.fontFootnote
            }
            PlainLabel {
                visible: text.length > 0
                text: usageLine.detail
                color: usageLine.look.inkMuted
                font.pixelSize: KodosiTheme.fontCaption
                font.features: { "tnum": 1 }
            }
        }
    }

    Row {
        id: row

        anchors.verticalCenter: parent.verticalCenter
        spacing: 12

        AbstractButton {
            id: modelKey

            readonly property bool opens: actions.changesAgent

            anchors.verticalCenter: parent.verticalCenter
            visible: root.model.length > 0
            implicitWidth: modelFace.implicitWidth
            implicitHeight: 26
            hoverEnabled: true
            activeFocusOnTab: opens
            objectName: "stage.tile." + root.sessionId + ".chat.model"
            Accessible.role: opens ? Accessible.ButtonMenu : Accessible.StaticText
            Accessible.name: qsTr("Model")
            Accessible.description: root.level >= 0 ? root.modelName + ", " + Models.DesktopSettings.effortLabel(root.facts.effort) : root.modelName
            ToolTip.visible: hovered && !actions.visible
            ToolTip.text: barsHover.hovered ? Models.DesktopSettings.effortLabel(root.facts.effort) : qsTr("Model")
            ToolTip.delay: Tip.delay
            onClicked: root.openModel()

            background: Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                radius: KodosiTheme.radiusXs
                color: "transparent"
                border.width: modelKey.visualFocus ? 1.5 : 0
                border.color: root.look.accentStrong
            }
            contentItem: Item {
                Row {
                    id: modelFace

                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 5

                    PlainLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.modelName
                        color: modelKey.opens && (modelKey.hovered || actions.visible) ? root.look.ink : root.look.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote

                        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                    }
                    EffortBars {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.level >= 0
                        level: root.level
                        count: root.levels.length
                        barHeight: 10
                        barWidth: 2
                        lit: root.look.accent
                        unlit: KodosiTheme.alpha(root.look.ink, 0.2)

                        HoverHandler { id: barsHover }
                    }
                    PlainLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.level < 0 && (root.facts.effort || "").length > 0
                        text: root.facts.effort || ""
                        color: root.look.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                    }
                    KIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: modelKey.opens
                        width: 10
                        height: 10
                        name: "chevrons"
                        strokeWidth: 2.6
                        color: root.look.inkFaint
                    }
                }
            }

            TerminalActions {
                id: actions

                y: -height - 6
                agentOnly: true
                session: root.session
                sessionId: root.sessionId
                identifier: "stage.tile." + root.sessionId
            }
        }
        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: (root.facts.mode || "").length > 0
            tip: root.facts.modeLabel || ""
            look: root.look

            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 13
                height: 13
                name: root.facts.modeIcon || "hand"
                strokeWidth: 2.2
                color: root.look.inkMuted
            }
            PlainLabel {
                id: modeWords

                anchors.verticalCenter: parent.verticalCenter
                visible: root.full
                text: root.facts.modeLabel || ""
                color: root.look.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
                Accessible.ignored: true
            }
        }
        AbstractButton {
            id: usageKey

            readonly property bool context: root.facts.context !== undefined
            readonly property bool limits: (root.facts.limits || []).length > 0

            anchors.verticalCenter: parent.verticalCenter
            visible: context || limits || (root.facts.cost || "").length > 0
            implicitWidth: usageFace.implicitWidth + 12
            implicitHeight: 26
            hoverEnabled: true
            activeFocusOnTab: true
            objectName: "chat.usage.key"
            Accessible.role: Accessible.ButtonMenu
            Accessible.name: qsTr("Usage")
            ToolTip.visible: hovered && !usage.visible
            ToolTip.text: qsTr("Usage")
            ToolTip.delay: Tip.delay
            onClicked: {
                if (usage.visible)
                    usage.close()
                else
                    root.openUsage()
            }

            background: Rectangle {
                radius: KodosiTheme.radiusSm
                color: usageKey.hovered || usage.visible ? KodosiTheme.alpha(root.look.ink, 0.07) : "transparent"
                border.width: usageKey.visualFocus ? 1.5 : 0
                border.color: root.look.accentStrong

                Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
            }
            contentItem: Item {
                Row {
                    id: usageFace

                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8

                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: usageKey.context
                        spacing: 4

                        UsageRing {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 14
                            height: 14
                            lineWidth: 2
                            look: root.look
                            limit: ({ used: root.facts.context ?? 0, level: root.facts.contextLevel || "normal" })
                        }
                        PlainLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("%1%").arg(root.facts.context ?? 0)
                            color: root.facts.contextLevel === "reached" ? root.look.danger
                                : root.facts.contextLevel === "near" ? root.look.caution : root.look.inkMuted
                            font.pixelSize: KodosiTheme.fontFootnote
                            font.features: { "tnum": 1 }
                        }
                    }
                    UsageDial {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !!root.facts.limit
                        limit: root.facts.limit || null
                        size: 20
                        look: root.look
                        Accessible.ignored: true
                    }
                    PlainLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !usageKey.context && !usageKey.limits
                        text: root.facts.cost || ""
                        color: root.look.inkMuted
                        font.pixelSize: KodosiTheme.fontFootnote
                        font.features: { "tnum": 1 }
                    }
                }
            }

            KPopover {
                id: usage

                y: -height - 6
                width: Math.max(240 + leftPadding + rightPadding, implicitWidth)
                padding: 14
                focus: true
                closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                objectName: "chat.usage"
                onAboutToShow: root.now = Date.now()

                contentItem: Column {
                    spacing: 12

                    UsageLine {
                        visible: usageKey.context
                        title: qsTr("Context: %1% used").arg(root.facts.context ?? 0)
                        detail: root.facts.tokens || ""
                        look: root.look

                        UsageRing {
                            anchors.centerIn: parent
                            width: 20
                            height: 20
                            lineWidth: 2.5
                            look: root.look
                            limit: ({ used: root.facts.context ?? 0, level: root.facts.contextLevel || "normal" })
                        }
                    }
                    Repeater {
                        model: root.facts.limits || []

                        UsageLine {
                            id: limitLine

                            required property var modelData
                            readonly property string resets: Models.Launcher.timeToReset(modelData, root.now)

                            title: modelData.spoken || ""
                            detail: resets.length > 0 ? qsTr("Resets in %1").arg(resets) : ""
                            look: root.look

                            UsageDial {
                                anchors.centerIn: parent
                                limit: limitLine.modelData
                                size: 22
                                look: root.look
                                Accessible.ignored: true
                            }
                        }
                    }
                    UsageLine {
                        visible: (root.facts.cost || "").length > 0
                        title: qsTr("This conversation")
                        detail: root.facts.cost || ""
                        look: root.look

                        Rectangle {
                            anchors.centerIn: parent
                            width: 18
                            height: 18
                            radius: 9
                            color: "transparent"
                            border.width: 1.5
                            border.color: root.look.inkMuted

                            PlainLabel {
                                anchors.centerIn: parent
                                text: "$"
                                color: root.look.inkMuted
                                font.pixelSize: KodosiTheme.fontCaption
                                font.weight: Font.DemiBold
                            }
                        }
                    }
                }
            }
        }
        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: (root.facts.helpers ?? 0) > 0
            tip: qsTr("%n helpers work", "", root.facts.helpers ?? 0)
            look: root.look

            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 14
                height: 14
                name: "people"
                strokeWidth: 2
                color: root.look.inkMuted
            }
            PlainLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: String(root.facts.helpers ?? 0)
                color: root.look.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
            }
        }
        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: (root.facts.background ?? 0) > 0
            tip: qsTr("%n commands run in the background", "", root.facts.background ?? 0)
            look: root.look

            KIcon {
                anchors.verticalCenter: parent.verticalCenter
                width: 13
                height: 13
                name: "refresh"
                strokeWidth: 2.2
                color: root.look.inkMuted
            }
            PlainLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: String(root.facts.background ?? 0)
                color: root.look.inkMuted
                font.pixelSize: KodosiTheme.fontFootnote
            }
        }
    }
}
