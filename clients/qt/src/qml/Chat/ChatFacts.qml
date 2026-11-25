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
    readonly property string effortWords: level >= 0 ? Models.DesktopSettings.effortLabel(facts.effort) : facts.effort || ""
    readonly property string modeWords: (facts.mode || "").length > 0 ? facts.modeLabel || "" : ""
    readonly property bool context: facts.context !== undefined
    readonly property var limit: facts.limit || null
    readonly property bool useShown: context || limit !== null || (facts.cost || "").length > 0
    readonly property real modelWords: modeWords.length > 0 ? modeLabel.wordWidth : 0
    readonly property real useWords: useShown ? (context ? contextWords.implicitWidth - contextPercent.implicitWidth : 0)
        + (limit ? limitWords.implicitWidth - limitPercent.implicitWidth : 0) : 0
    readonly property var fitInput: [width, modelPart.visible, modelPart.implicitWidth, usePart.visible, usePart.implicitWidth,
        modelWords, useWords]
    property string form: "line"
    readonly property bool named: form !== "short"
    property double now: Date.now()

    function fit(): void {
        if (width <= 0)
            return
        const short = form === "short"
        const model = modelPart.visible ? modelPart.implicitWidth + (short ? modelWords : 0) : 0
        const use = usePart.visible ? usePart.implicitWidth + (short ? useWords : 0) : 0
        const room = short ? width - 1 : width
        form = model + use + (model > 0 && use > 0 ? grid.columnSpacing : 0) <= room ? "line"
            : Math.max(model, use) <= room ? "lines" : "short"
    }
    function levelInk(level: string): color {
        return level === "reached" ? look.danger : level === "near" ? look.caution : look.ink
    }
    function loadModels(): void {
        if (launcher && agent.length > 0 && models === null)
            launcher.loadModels(agent)
    }
    function openModel(): void {
        if (modelKey.visible)
            actions.open()
    }
    function openUsage(): void {
        if (usageKey.visible)
            usage.open()
    }

    implicitHeight: Math.max(26, grid.implicitHeight)
    objectName: "chat.facts"

    Component.onCompleted: {
        if (launcher)
            launcher.refresh(false)
        loadModels()
    }
    onAgentChanged: loadModels()
    onLauncherChanged: loadModels()
    onFitInputChanged: fit()

    Connections {
        function onAgentsChanged() {
            root.loadModels()
        }

        target: root.launcher
    }

    component ModeMark: Row {
        id: mark

        property string icon: ""
        property string word: ""
        property bool named: true
        property color ink: KodosiTheme.look.ink
        readonly property real wordWidth: wordLabel.implicitWidth + spacing
        readonly property bool hovered: markHover.hovered

        spacing: 4
        Accessible.role: Accessible.StaticText
        Accessible.name: word

        HoverHandler { id: markHover }
        KIcon {
            anchors.verticalCenter: parent.verticalCenter
            width: 13
            height: 13
            name: mark.icon || "hand"
            strokeWidth: 2.2
            color: mark.ink
        }
        PlainLabel {
            id: wordLabel

            anchors.verticalCenter: parent.verticalCenter
            visible: mark.named
            text: mark.word
            color: mark.ink
            font.pixelSize: KodosiTheme.fontCallout
            font.weight: Font.Medium
            Accessible.ignored: true
        }
    }
    component ModelFace: Row {
        id: face

        property Look look: KodosiTheme.look
        property color ink: look.ink
        property string name: ""
        property int level: -1
        property int levels: 0
        property string effort: ""
        property string modeIcon: ""
        property string modeWord: ""
        property bool named: true
        readonly property real wordWidth: modeWord.length > 0 ? mode.wordWidth : 0
        readonly property bool barsHovered: barsHover.hovered
        readonly property bool modeHovered: mode.hovered

        spacing: 6

        PlainLabel {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 160)
            text: face.name
            color: face.ink
            font.pixelSize: KodosiTheme.fontCallout
            font.weight: Font.Medium
            elide: Text.ElideMiddle
        }
        EffortBars {
            anchors.verticalCenter: parent.verticalCenter
            visible: face.level >= 0
            level: face.level
            count: face.levels
            barHeight: 10
            barWidth: 2
            lit: face.look.accent
            unlit: KodosiTheme.alpha(face.look.ink, 0.2)

            HoverHandler { id: barsHover }
        }
        PlainLabel {
            anchors.verticalCenter: parent.verticalCenter
            visible: face.level < 0 && face.effort.length > 0
            text: face.effort
            color: face.ink
            font.pixelSize: KodosiTheme.fontCallout
            font.weight: Font.Medium
        }
        PlainLabel {
            anchors.verticalCenter: parent.verticalCenter
            visible: face.modeWord.length > 0
            text: "·"
            color: face.look.inkFaint
            font.pixelSize: KodosiTheme.fontCallout
            font.weight: Font.Medium
            Accessible.ignored: true
        }
        ModeMark {
            id: mode

            anchors.verticalCenter: parent.verticalCenter
            visible: face.modeWord.length > 0
            icon: face.modeIcon
            word: face.modeWord
            named: face.named
            ink: face.ink
        }
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

    Flickable {
        id: strip

        anchors.fill: parent
        anchors.topMargin: -2
        anchors.bottomMargin: -2
        contentWidth: grid.implicitWidth
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentWidth > width + 0.5
        clip: interactive

        onInteractiveChanged: {
            if (!interactive)
                returnToBounds()
        }

        Grid {
            id: grid

            y: 2
            columns: root.form === "lines" ? 1 : 2
            columnSpacing: 8
            rowSpacing: 6
            verticalItemAlignment: Grid.AlignVCenter

            Row {
                id: modelPart

                visible: root.model.length > 0 || root.modeWords.length > 0

                PickerKey {
                    id: modelKey

                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.model.length > 0 && actions.changesAgent
                    look: root.look
                    open: actions.visible
                    accessoryShown: true
                    objectName: "stage.tile." + root.sessionId + ".chat.model"
                    Accessible.name: qsTr("Model")
                    Accessible.description: [root.modelName, root.effortWords, root.modeWords].filter(part => part.length > 0).join(", ")
                    ToolTip.visible: hovered && !actions.visible
                    ToolTip.text: keyFace.barsHovered ? root.effortWords : keyFace.modeHovered && !root.named ? root.modeWords : qsTr("Model")
                    ToolTip.delay: Tip.delay
                    accessory: ModelFace {
                        id: keyFace

                        look: root.look
                        ink: modelKey.open ? root.look.accentStrong : root.look.ink
                        name: root.modelName
                        level: root.level
                        levels: root.levels.length
                        effort: root.facts.effort || ""
                        modeIcon: root.facts.modeIcon || ""
                        modeWord: root.modeWords
                        named: root.named
                    }
                    onClicked: root.openModel()

                    TerminalActions {
                        id: actions

                        y: -height - 6
                        agentOnly: true
                        session: root.session
                        sessionId: root.sessionId
                        identifier: "stage.tile." + root.sessionId
                    }
                }
                ModelFace {
                    id: plainFace

                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.model.length > 0 && !actions.changesAgent
                    leftPadding: 4
                    rightPadding: 4
                    look: root.look
                    ink: root.look.inkMuted
                    name: root.modelName
                    level: root.level
                    levels: root.levels.length
                    effort: root.facts.effort || ""
                    modeIcon: root.facts.modeIcon || ""
                    modeWord: root.modeWords
                    named: root.named
                    Accessible.role: Accessible.StaticText
                    Accessible.name: [root.modelName, root.effortWords, root.modeWords].filter(part => part.length > 0).join(", ")
                }
                ModeMark {
                    id: modeLabel

                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.model.length === 0 && root.modeWords.length > 0
                    leftPadding: 4
                    rightPadding: 4
                    icon: root.facts.modeIcon || ""
                    word: root.modeWords
                    named: root.named
                    ink: root.look.inkMuted
                    ToolTip.visible: hovered && !root.named
                    ToolTip.text: root.modeWords
                    ToolTip.delay: Tip.delay
                }
            }
            Row {
                id: usePart

                visible: root.useShown || (root.facts.background ?? 0) > 0
                spacing: 8

                PickerKey {
                    id: usageKey

                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.useShown
                    look: root.look
                    open: usage.visible
                    accessoryShown: true
                    objectName: "chat.usage.key"
                    Accessible.name: qsTr("Usage")
                    Accessible.description: [root.context ? qsTr("Context: %1% used").arg(root.facts.context) : "",
                        root.limit ? root.limit.spoken || "" : "", root.facts.cost || ""].filter(part => part.length > 0).join(", ")
                    ToolTip.visible: hovered && !usage.visible
                    ToolTip.text: qsTr("Usage")
                    ToolTip.delay: Tip.delay
                    accessory: Row {
                        spacing: 6

                        Row {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.context
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
                                id: contextWords

                                anchors.verticalCenter: parent.verticalCenter
                                visible: root.named
                                text: qsTr("%1% context").arg(root.facts.context ?? 0)
                                color: root.levelInk(root.facts.contextLevel || "")
                                font.pixelSize: KodosiTheme.fontCallout
                                font.weight: Font.Medium
                                font.features: { "tnum": 1 }
                            }
                            PlainLabel {
                                id: contextPercent

                                anchors.verticalCenter: parent.verticalCenter
                                visible: !root.named
                                text: qsTr("%1%").arg(root.facts.context ?? 0)
                                color: root.levelInk(root.facts.contextLevel || "")
                                font.pixelSize: KodosiTheme.fontCallout
                                font.weight: Font.Medium
                                font.features: { "tnum": 1 }
                            }
                        }
                        Row {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.limit !== null
                            spacing: 4

                            UsageRing {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 14
                                height: 14
                                lineWidth: 2
                                look: root.look
                                limit: root.limit
                            }
                            PlainLabel {
                                id: limitWords

                                anchors.verticalCenter: parent.verticalCenter
                                visible: root.named
                                text: qsTr("%1 %2%").arg(root.limit ? root.limit.window : "").arg(root.limit ? root.limit.used : 0)
                                color: root.levelInk(root.limit ? root.limit.level : "")
                                font.pixelSize: KodosiTheme.fontCallout
                                font.weight: Font.Medium
                                font.features: { "tnum": 1 }
                            }
                            PlainLabel {
                                id: limitPercent

                                anchors.verticalCenter: parent.verticalCenter
                                visible: !root.named
                                text: qsTr("%1%").arg(root.limit ? root.limit.used : 0)
                                color: root.levelInk(root.limit ? root.limit.level : "")
                                font.pixelSize: KodosiTheme.fontCallout
                                font.weight: Font.Medium
                                font.features: { "tnum": 1 }
                            }
                        }
                        PlainLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: !root.context && root.limit === null
                            text: root.facts.cost || ""
                            color: root.look.ink
                            font.pixelSize: KodosiTheme.fontCallout
                            font.weight: Font.Medium
                            font.features: { "tnum": 1 }
                        }
                    }
                    onClicked: {
                        if (usage.visible)
                            usage.close()
                        else
                            root.openUsage()
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
                                visible: root.context
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
                    visible: (root.facts.background ?? 0) > 0
                    tip: qsTr("%n commands run in the background", "", root.facts.background ?? 0)
                    look: root.look

                    KIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 14
                        height: 14
                        name: "refresh"
                        strokeWidth: 2.2
                        color: root.look.inkMuted
                    }
                    PlainLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: String(root.facts.background ?? 0)
                        color: root.look.inkMuted
                        font.pixelSize: KodosiTheme.fontCallout
                    }
                }
            }
        }
    }
}
