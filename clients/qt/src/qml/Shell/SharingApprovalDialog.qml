pragma ComponentBehavior: Bound
import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Shapes
import Kodosi.Models 1.0 as Models

KDialog {
    id: root

    readonly property var question: Models.Sharing.question
    property var shown: ({})
    property bool closing: false
    property real rise: 0
    property bool arrived: false
    property bool read: false
    property real readLeft: 1
    property double readFrom: 0
    readonly property int readSeconds: shown.readSeconds ?? 0
    readonly property bool watched: opened && Application.state === Qt.ApplicationActive
    readonly property bool allowed: consent.checked && read

    function follow(): void {
        if (closing)
            return;
        if (question.approvalId === undefined) {
            if (visible)
                leaving.restart();
            return;
        }
        leaving.stop();
        if (visible && shown.approvalId === question.approvalId)
            return;
        shown = question;
        settling.restart();
        consent.checked = false;
        read = readSeconds <= 0;
        countdown();
        arrived = false;
        (scroll.contentItem as Flickable).contentY = 0;
        if (!visible)
            open();
        if (Application.state === Qt.ApplicationActive)
            arrive();
        heading.Accessible.announce([shown.title, shown.asker, shown.message].filter(words => words.length > 0).join(" "));
    }
    function answer(allow: bool): void {
        if (settling.running || (allow && !allowed))
            return;
        Models.Sharing.answer(shown.approvalId, allow);
        if (question.approvalId === undefined) {
            leaving.stop();
            close();
        }
    }
    function focusCancel(): void {
        if (opened && !settling.running)
            cancel.forceActiveFocus();
    }
    // The read time counts only while the owner can see the question, and from the start each time.
    function countdown(): void {
        reading.stop();
        readLeft = read ? 0 : 1;
        readFrom = Date.now();
        if (watched && !read)
            reading.start();
    }
    function arrive(): void {
        arrived = true;
        if (KodosiTheme.reduceMotion)
            arrival.complete();
        else
            arrival.restart();
    }

    // The question ends only with an answer, and Escape answers Cancel: neither a click outside nor
    // a notification that brings a terminal to the front closes it.
    closePolicy: Popup.NoAutoClose
    title: shown.title ?? ""
    objectName: "sharing.question"
    width: Math.min(440, parent ? parent.width - 48 : 440)
    height: Math.min(implicitHeight, parent ? parent.height - 48 : implicitHeight)
    y: parent ? (parent.height - height) / 2 + rise : 0
    padding: 24
    topPadding: 24
    header: null
    footer: null
    enter: Transition {
        // Reduced motion keeps the fade: it moves nothing.
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
        NumberAnimation { property: "scale"; from: 0.95; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
        NumberAnimation { property: "rise"; from: 18; to: 0; duration: KodosiTheme.motionSpring; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve }
    }
    onQuestionChanged: follow()
    onWatchedChanged: {
        if (watched && !arrived)
            arrive();
        countdown();
    }
    onAboutToShow: closing = false
    onAboutToHide: closing = true
    onOpened: focusCancel()
    onClosed: {
        closing = false;
        follow();
    }

    Shortcut {
        enabled: root.opened && !settling.running
        sequences: [StandardKey.Cancel]
        onActivated: root.answer(false)
    }
    // A click or a key meant for what was there a moment ago does not answer a new question.
    Timer {
        id: settling
        interval: 500
        onTriggered: root.focusCancel()
    }
    // A question that the host sends again right away, after it connects or resends its state,
    // stays on screen.
    Timer {
        id: leaving
        interval: 300
        onTriggered: root.close()
    }
    // Frames can come faster than the screen's rate, as on a display without vsync, so the read time
    // follows the clock and not the animation time.
    FrameAnimation {
        id: reading
        onTriggered: {
            root.readLeft = Math.max(0, Math.min(1, 1 - (Date.now() - root.readFrom) / (root.readSeconds * 1000)));
            if (root.readLeft === 0) {
                reading.stop();
                root.read = true;
            }
        }
    }
    SequentialAnimation {
        id: arrival

        PropertyAction { target: mark; property: "scale"; value: 0.6 }
        PropertyAction { target: shield; property: "rotation"; value: 0 }
        PauseAnimation { duration: 100 }
        NumberAnimation { target: mark; property: "scale"; to: 1; duration: KodosiTheme.motionSnappy; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
        NumberAnimation { target: shield; property: "rotation"; to: 8; duration: 62; easing.type: Easing.InOutSine }
        NumberAnimation { target: shield; property: "rotation"; to: -8; duration: 125; easing.type: Easing.InOutSine }
        NumberAnimation { target: shield; property: "rotation"; to: 8; duration: 125; easing.type: Easing.InOutSine }
        NumberAnimation { target: shield; property: "rotation"; to: -8; duration: 125; easing.type: Easing.InOutSine }
        NumberAnimation { target: shield; property: "rotation"; to: 0; duration: 63; easing.type: Easing.InOutSine }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 14

        KScrollView {
            id: scroll

            function reveal(item: Item): void {
                const view = contentItem as Flickable;
                const top = item.mapToItem(view.contentItem, 0, 0).y;
                view.contentY = Math.max(Math.min(view.contentY, top), top + item.height - view.height);
            }

            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth

            ColumnLayout {
                width: scroll.availableWidth
                spacing: 14

                Item {
                    id: mark

                    Layout.preferredWidth: 44
                    Layout.preferredHeight: 44
                    Accessible.ignored: true

                    Squircle {
                        anchors.fill: parent
                        lightColor: KodosiTheme.caution
                        darkColor: KodosiTheme.caution
                        rim: 0.75
                        shadow: KodosiTheme.isDark ? 0.4 : 0.18
                        shadowRadius: width * 0.08
                        shadowX: 1
                        shadowY: 2
                    }
                    Shape {
                        id: shield

                        anchors.centerIn: parent
                        width: 24
                        height: 24
                        // The shield fills 19 of the grid's 24 units, and shows about 24 high.
                        scale: 24 / 19
                        preferredRendererType: Shape.CurveRenderer

                        ShapePath {
                            strokeColor: "transparent"
                            fillColor: KodosiTheme.dangerInk
                            PathSvg { path: "M12 2.5 L19.5 5.5 V11 Q19.5 17.5 12 21.5 Q4.5 17.5 4.5 11 V5.5 Z" }
                        }
                        ShapePath {
                            strokeColor: KodosiTheme.caution
                            strokeWidth: 2.2
                            fillColor: "transparent"
                            capStyle: ShapePath.RoundCap
                            PathSvg { path: "M12 7.5 V12.5" }
                        }
                        ShapePath {
                            strokeColor: "transparent"
                            fillColor: KodosiTheme.caution
                            PathAngleArc { centerX: 12; centerY: 16; radiusX: 1.2; radiusY: 1.2; sweepAngle: 360 }
                        }
                    }
                }
                PlainLabel {
                    id: heading

                    Layout.fillWidth: true
                    text: root.shown.title ?? ""
                    wrapMode: Text.Wrap
                    font.pixelSize: KodosiTheme.fontTitle
                    font.weight: Font.DemiBold
                }
                RowLayout {
                    Layout.fillWidth: true
                    visible: asker.text.length > 0
                    spacing: 6

                    KIcon {
                        Layout.alignment: Qt.AlignTop
                        Layout.topMargin: Math.max(0, (askerLine.height - height) / 2)
                        Layout.preferredWidth: KodosiTheme.scaled(13)
                        Layout.preferredHeight: KodosiTheme.scaled(13)
                        name: "terminal"
                        color: KodosiTheme.caution
                    }
                    PlainLabel {
                        id: asker

                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignTop
                        text: root.shown.asker ?? ""
                        color: KodosiTheme.caution
                        wrapMode: Text.Wrap
                        font.pixelSize: KodosiTheme.fontFootnote
                        font.weight: Font.DemiBold
                    }
                    FontMetrics {
                        id: askerLine
                        font: asker.font
                    }
                }
                PlainLabel {
                    Layout.fillWidth: true
                    text: root.shown.message ?? ""
                    wrapMode: Text.Wrap
                }
                CheckBox {
                    id: consent

                    property real drawn: 0

                    objectName: "sharing.question.consent"
                    Accessible.id: objectName
                    Layout.fillWidth: true
                    implicitHeight: Math.max(44, implicitContentHeight + topPadding + bottomPadding)
                    leftPadding: 12
                    rightPadding: 12
                    topPadding: 10
                    bottomPadding: 10
                    spacing: 10
                    enabled: !settling.running
                    text: root.shown.consent ?? ""
                    indicator: null
                    onActiveFocusChanged: {
                        if (activeFocus)
                            scroll.reveal(consent);
                    }
                    onCheckedChanged: {
                        if (checked) {
                            drawing.restart();
                        } else {
                            drawing.stop();
                            drawn = 0;
                        }
                    }
                    background: Well {
                        radius: KodosiTheme.radiusMd

                        Rectangle {
                            anchors.fill: parent
                            visible: consent.visualFocus
                            radius: parent.radius
                            color: "transparent"
                            border.width: 1.5
                            border.color: KodosiTheme.accentStrong
                        }
                    }
                    contentItem: Item {
                        implicitWidth: consentRow.implicitWidth
                        implicitHeight: consentRow.implicitHeight

                        RowLayout {
                            id: consentRow

                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: consent.spacing

                            Rectangle {
                                id: tick

                                Layout.alignment: Qt.AlignTop
                                Layout.topMargin: Math.max(0, (consentLine.height - height) / 2)
                                Layout.preferredWidth: 20
                                Layout.preferredHeight: 20
                                radius: 5
                                color: consent.checked ? KodosiTheme.accent
                                    : KodosiTheme.mix(KodosiTheme.well, KodosiTheme.shadow, KodosiTheme.isDark ? 0.4 : 0.1)
                                border.width: consent.checked ? 0 : 1.5
                                border.color: KodosiTheme.inkFaint

                                Behavior on color { ColorAnimation { duration: KodosiTheme.motionHover } }

                                Item {
                                    width: tick.width * consent.drawn
                                    height: tick.height
                                    clip: true

                                    KIcon {
                                        width: tick.width
                                        height: tick.height
                                        name: "check"
                                        color: KodosiTheme.accentInk
                                        strokeWidth: 2.4
                                    }
                                }
                            }
                            PlainLabel {
                                id: consentText

                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignTop
                                Layout.topMargin: Math.max(0, (tick.height - consentLine.height) / 2)
                                text: consent.text
                                wrapMode: Text.Wrap
                                Accessible.ignored: true
                            }
                            FontMetrics {
                                id: consentLine
                                font: consentText.font
                            }
                        }
                    }

                    NumberAnimation {
                        id: drawing
                        target: consent
                        property: "drawn"
                        from: 0
                        to: 1
                        duration: KodosiTheme.motionFade
                        easing.type: Easing.OutCubic
                    }
                }
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 8

            KButton {
                id: cancel

                objectName: "sharing.question.cancel"
                Accessible.id: objectName
                variant: KButton.Secondary
                text: qsTranslate("QPlatformTheme", "Cancel")
                onClicked: root.answer(false)
            }
            KButton {
                id: confirm

                objectName: "sharing.question.confirm"
                Accessible.id: objectName
                variant: KButton.Danger
                enabled: root.allowed
                text: root.shown.confirm ?? ""
                rightPadding: leftPadding + clock.room
                onClicked: root.answer(true)
                // A platform theme can press the focused button with Return, and Return never allows.
                Keys.onReturnPressed: event => {
                    event.accepted = true;
                }
                Keys.onEnterPressed: event => {
                    event.accepted = true;
                }

                Item {
                    id: clock

                    property real presence: root.read ? 0 : 1
                    readonly property real room: (width + 7) * presence

                    anchors.right: parent.right
                    anchors.rightMargin: confirm.leftPadding
                    anchors.verticalCenter: parent.verticalCenter
                    width: KodosiTheme.scaled(18)
                    height: width
                    visible: presence > 0
                    opacity: presence
                    scale: 0.6 + 0.4 * presence
                    Accessible.ignored: true

                    Behavior on presence { NumberAnimation { duration: KodosiTheme.motionFade; easing.type: Easing.BezierSpline; easing.bezierCurve: KodosiTheme.outCurve } }

                    Shape {
                        anchors.fill: parent
                        preferredRendererType: Shape.CurveRenderer

                        ShapePath {
                            strokeColor: KodosiTheme.hairline
                            strokeWidth: 2
                            fillColor: "transparent"
                            PathAngleArc {
                                centerX: clock.width / 2
                                centerY: clock.height / 2
                                radiusX: clock.width / 2 - 1
                                radiusY: radiusX
                                sweepAngle: 360
                            }
                        }
                        ShapePath {
                            strokeColor: root.readLeft > 0 ? KodosiTheme.inkMuted : "transparent"
                            strokeWidth: 2
                            fillColor: "transparent"
                            capStyle: ShapePath.RoundCap
                            PathAngleArc {
                                centerX: clock.width / 2
                                centerY: clock.height / 2
                                radiusX: clock.width / 2 - 1
                                radiusY: radiusX
                                startAngle: 270 - 360 * root.readLeft
                                sweepAngle: 360 * root.readLeft
                            }
                        }
                    }
                    Text {
                        anchors.centerIn: parent
                        text: String(Math.ceil(root.readLeft * root.readSeconds))
                        textFormat: Text.PlainText
                        color: KodosiTheme.inkMuted
                        font.pixelSize: KodosiTheme.fontCaption
                        font.weight: Font.DemiBold
                        font.features: { "tnum": 1 }
                        Accessible.ignored: true
                    }
                }
            }
        }
    }
}
