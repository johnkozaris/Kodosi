pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

TextEdit {
    id: root

    property string placeholder: ""
    property bool holds: true
    property bool lit: true
    readonly property real cell: metrics.advanceWidth("M")
    readonly property real fitWidth: Math.ceil(leftPadding + cell + 4 + metrics.advanceWidth(placeholder) + rightPadding)

    signal submitted

    function wake() {
        lit = true
        holds = true
        blink.restart()
    }

    color: KodosiTheme.terminalInk
    selectionColor: KodosiTheme.alpha(KodosiTheme.terminalAccent, 0.35)
    selectedTextColor: KodosiTheme.terminalInk
    font.family: "monospace"
    font.pixelSize: KodosiTheme.fontBody
    font.weight: Font.Medium
    wrapMode: TextEdit.WrapAnywhere
    textFormat: TextEdit.PlainText
    selectByMouse: true
    cursorVisible: true
    topPadding: 4.5
    bottomPadding: 4.5
    rightPadding: cell
    activeFocusOnTab: true
    Accessible.role: Accessible.EditableText
    Accessible.description: placeholder

    onTextChanged: {
        const line = text.replace(/[\r\n\t\u2028\u2029]+/g, " ")
        if (line !== text) {
            const position = cursorPosition
            text = line
            cursorPosition = Math.min(position, line.length)
        }
        wake()
    }
    onCursorPositionChanged: wake()
    onActiveFocusChanged: wake()

    Keys.onReturnPressed: root.submitted()
    Keys.onEnterPressed: root.submitted()
    Keys.onTabPressed: nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocusReason)
    Keys.onBacktabPressed: nextItemInFocusChain(false).forceActiveFocus(Qt.BacktabFocusReason)
    Keys.onEscapePressed: event => {
        if (length > 0)
            clear()
        else
            event.accepted = false
    }

    FontMetrics {
        id: metrics
        font: root.font
    }
    Timer {
        id: blink
        interval: root.holds ? 1100 : root.lit ? 560 : 440
        repeat: true
        running: root.activeFocus && root.visible && !KodosiTheme.reduceMotion
        onTriggered: {
            root.holds = false
            root.lit = !root.lit
        }
    }
    Text {
        x: root.leftPadding + root.cell + 4
        y: root.topPadding
        visible: root.length === 0
        text: root.placeholder
        textFormat: Text.PlainText
        color: KodosiTheme.terminalInkFaint
        font: root.font
    }

    cursorDelegate: Item {
        width: root.cell
        height: root.cursorRectangle.height

        Rectangle {
            anchors.fill: parent
            anchors.topMargin: 1
            anchors.bottomMargin: 1
            radius: 1.5
            visible: !root.activeFocus || root.lit
            color: root.activeFocus ? KodosiTheme.terminalAccent : "transparent"
            border.width: root.activeFocus ? 0 : 1.5
            border.color: KodosiTheme.alpha(KodosiTheme.terminalAccent, 0.85)
        }
        Text {
            visible: root.activeFocus && root.lit
            text: root.cursorPosition < root.length ? root.getText(root.cursorPosition, root.cursorPosition + 1) : ""
            textFormat: Text.PlainText
            color: KodosiTheme.terminal
            font: root.font
        }
    }
}
