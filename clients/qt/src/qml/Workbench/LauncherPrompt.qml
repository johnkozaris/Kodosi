pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick

TextEdit {
    id: root

    property Look look: KodosiTheme.terminalLook
    property string hint: ""
    property bool clearsOnEscape: true
    property bool holds: true
    property bool lit: true
    readonly property real cell: cellMetrics.advanceWidth
    readonly property real fitWidth: Math.ceil(leftPadding + hintMetrics.advanceWidth + rightPadding)

    signal submitted

    function wake(): void {
        lit = true
        holds = true
        blink.restart()
    }

    color: root.look.ink
    selectionColor: KodosiTheme.alpha(root.look.accent, 0.35)
    selectedTextColor: root.look.ink
    font.family: "monospace"
    font.pixelSize: KodosiTheme.fontBody
    font.weight: Font.Medium
    wrapMode: TextEdit.WrapAnywhere
    textFormat: TextEdit.PlainText
    selectByMouse: true
    cursorVisible: true
    topPadding: Math.max(0, (KodosiTheme.scaled(26) - lineMetrics.height) / 2)
    bottomPadding: topPadding
    rightPadding: cell
    activeFocusOnTab: true
    Accessible.role: Accessible.EditableText
    Accessible.description: hint

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
        if (clearsOnEscape && length > 0)
            clear()
        else
            event.accepted = false
    }

    FontMetrics {
        id: lineMetrics
        font: root.font
    }
    TextMetrics {
        id: cellMetrics
        font: root.font
        text: "M"
    }
    TextMetrics {
        id: hintMetrics
        font: root.font
        text: root.hint
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
        x: root.leftPadding
        y: root.topPadding
        visible: root.length === 0
        text: root.hint
        textFormat: Text.PlainText
        color: root.look.inkFaint
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
            color: root.activeFocus ? root.look.accent : "transparent"
            border.width: root.activeFocus ? 0 : 1.5
            border.color: KodosiTheme.alpha(root.look.accent, 0.85)
        }
        Text {
            visible: root.activeFocus && root.lit
            text: root.cursorPosition < root.length ? root.getText(root.cursorPosition, root.cursorPosition + 1)
                : root.length === 0 ? root.hint.charAt(0) : ""
            textFormat: Text.PlainText
            color: root.look.terminal
            font: root.font
        }
    }
}
