pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls.Basic
import Kodosi.Models 1.0 as Models

// The marks stay still, because the agent's own mark carries the light of work.
Item {
    id: root

    property var session: null
    readonly property int count: session ? session.helpers ?? 0 : 0
    readonly property bool working: count > 0
    readonly property color tint: Identity.accountTint(Models.DesktopSettings.agentOf(session ? session.program || "" : ""),
        session && session.programAccount !== undefined ? session.programAccount : -1)
    property bool ready: false

    implicitWidth: marks.implicitWidth
    implicitHeight: marks.implicitHeight
    visible: working
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("%n helpers work", "", count)
    Accessible.ignored: !visible
    ToolTip.visible: hover.hovered
    ToolTip.text: Accessible.name
    ToolTip.delay: Tip.delay

    Component.onCompleted: ready = true
    onWorkingChanged: {
        if (ready && working && !KodosiTheme.reduceMotion)
            arrive.restart()
    }

    HoverHandler { id: hover }
    Row {
        id: marks

        spacing: 2

        Repeater {
            model: Math.min(root.count, 3)

            Rectangle {
                width: 6
                height: 6
                radius: 2
                color: root.tint
            }
        }
    }
    NumberAnimation { id: arrive; target: marks; property: "scale"; from: 0.6; to: 1; duration: KodosiTheme.motionSpring; easing.type: Easing.OutBack; easing.overshoot: KodosiTheme.overshoot }
}
