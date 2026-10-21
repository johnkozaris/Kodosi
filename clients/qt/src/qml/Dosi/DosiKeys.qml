pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

Row {
    id: root

    required property string sessionId
    readonly property var question: Models.DesktopState.dosiOn ? Models.Dosi.terminalQuestions[sessionId] || null : null
    readonly property bool brief: !!question && question.multiple !== true && question.options.length <= 3
        && question.options.every(option => option.length <= 12)

    spacing: 4
    visible: opacity > 0
    opacity: question ? 1 : 0
    scale: question ? 1 : 0.9
    Accessible.role: Accessible.Grouping
    Accessible.name: question ? question.question : ""

    Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionSnappy } }
    Behavior on scale { NumberAnimation { duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot } }

    DosiFace {
        anchors.verticalCenter: parent.verticalCenter
        size: 16
        mood: "asking"

        HoverHandler { id: faceHover }
        ToolTip.visible: faceHover.hovered
        ToolTip.text: root.question ? root.question.question : ""
        ToolTip.delay: Tip.delay
    }
    KButton {
        anchors.verticalCenter: parent.verticalCenter
        visible: !!root.question && !root.brief
        compact: true
        variant: KButton.Tinted
        text: qsTr("Answer")
        onClicked: Models.DesktopState.openDosi()
    }
    Repeater {
        model: root.brief ? root.question.options : []

        KButton {
            required property string modelData

            anchors.verticalCenter: parent.verticalCenter
            compact: true
            text: modelData
            objectName: "dosi.keys." + modelData
            Accessible.id: objectName
            onClicked: Models.Dosi.answer(root.question.item, [modelData])
        }
    }
}
