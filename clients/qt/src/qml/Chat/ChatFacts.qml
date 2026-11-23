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
    readonly property bool full: row.implicitWidth + 10 + tail.implicitWidth <= width

    function loadModels(): void {
        if (launcher && agent.length > 0 && models === null)
            launcher.loadModels(agent)
    }

    implicitHeight: 20
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
        implicitHeight: 20
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

    Row {
        id: row

        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

        AbstractButton {
            id: modelKey

            readonly property bool opens: actions.changesAgent

            anchors.verticalCenter: parent.verticalCenter
            visible: root.model.length > 0
            implicitWidth: modelFace.implicitWidth
            implicitHeight: 20
            hoverEnabled: true
            activeFocusOnTab: opens
            objectName: "stage.tile." + root.sessionId + ".chat.model"
            Accessible.role: opens ? Accessible.ButtonMenu : Accessible.StaticText
            Accessible.name: qsTr("Model")
            Accessible.description: root.level >= 0 ? root.modelName + ", " + Models.DesktopSettings.effortLabel(root.facts.effort) : root.modelName
            ToolTip.visible: hovered && !actions.visible
            ToolTip.text: barsHover.hovered ? Models.DesktopSettings.effortLabel(root.facts.effort) : qsTr("Model")
            ToolTip.delay: Tip.delay
            onClicked: {
                if (opens)
                    actions.open()
            }

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
                        color: modelKey.opens && (modelKey.hovered || actions.visible) ? root.look.ink : root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption

                        Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }
                    }
                    EffortBars {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.level >= 0
                        level: root.level
                        count: root.levels.length
                        barHeight: 9
                        barWidth: 2
                        lit: root.look.accent
                        unlit: KodosiTheme.alpha(root.look.ink, 0.2)

                        HoverHandler { id: barsHover }
                    }
                    PlainLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.level < 0 && (root.facts.effort || "").length > 0
                        text: root.facts.effort || ""
                        color: root.look.inkFaint
                        font.pixelSize: KodosiTheme.fontCaption
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
                width: 11
                height: 11
                name: root.facts.modeIcon || "hand"
                strokeWidth: 2.2
                color: root.look.inkFaint
            }
        }
        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.facts.context !== undefined
            tip: qsTr("Context: %1% used").arg(root.facts.context ?? 0)
            look: root.look

            UsageRing {
                anchors.verticalCenter: parent.verticalCenter
                width: 11
                height: 11
                lineWidth: 1.75
                look: root.look
                limit: ({ used: root.facts.context ?? 0, level: root.facts.contextLevel || "normal" })
            }
            PlainLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("%1%").arg(root.facts.context ?? 0)
                color: root.facts.contextLevel === "reached" ? root.look.danger
                    : root.facts.contextLevel === "near" ? root.look.caution : root.look.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
            }
        }
        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: (root.facts.helpers ?? 0) > 0
            tip: qsTr("%n helpers work", "", root.facts.helpers ?? 0)
            look: root.look

            KIcon {
                width: 12
                height: 12
                name: "people"
                strokeWidth: 2
                color: root.look.inkFaint
            }
            PlainLabel {
                text: String(root.facts.helpers ?? 0)
                color: root.look.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
            }
        }
        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: (root.facts.background ?? 0) > 0
            tip: qsTr("%n commands run in the background", "", root.facts.background ?? 0)
            look: root.look

            KIcon {
                width: 11
                height: 11
                name: "refresh"
                strokeWidth: 2.2
                color: root.look.inkFaint
            }
            PlainLabel {
                text: String(root.facts.background ?? 0)
                color: root.look.inkFaint
                font.pixelSize: KodosiTheme.fontCaption
            }
        }
    }
    Row {
        id: tail

        anchors.left: row.right
        anchors.leftMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        visible: root.full
        spacing: 10

        Fact {
            anchors.verticalCenter: parent.verticalCenter
            visible: !!root.facts.limit
            tip: root.facts.limit ? Models.Launcher.spokenWithReset(root.facts.limit, Date.now()) : ""
            look: root.look

            UsageDial {
                limit: root.facts.limit || null
                size: 18
                look: root.look
            }
        }
        PlainLabel {
            anchors.verticalCenter: parent.verticalCenter
            visible: text.length > 0
            text: root.facts.cost || ""
            color: root.look.inkFaint
            font.pixelSize: KodosiTheme.fontCaption
            font.features: { "tnum": 1 }
        }
    }
}
