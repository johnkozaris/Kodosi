pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Layouts

// What a canvas video shows over its picture: nothing while the picture is live, a dot while the last
// picture rests and a word once that lasts, a spinner while no picture came yet, and in place of the
// picture one line, with a way to try again only for what went wrong unexpectedly.
Item {
    id: root

    // What the video shows, as StreamVideo tells it.
    property string showing: ""
    // The terminal's computer.
    property string computer: ""
    // The line for a video that failed.
    property string failure: ""
    // Why it failed, for assistive technology.
    property string problem: ""
    readonly property string line: {
        switch (showing) {
        case "sharingOff": return qsTr("Sharing is off on %1").arg(computer);
        case "offline": return qsTr("%1 is offline").arg(computer);
        case "locked": return qsTr("%1 is locked").arg(computer);
        case "anotherAccount": return qsTr("%1 is on another account").arg(computer);
        case "asleep": return qsTr("%1 is asleep").arg(computer);
        case "recording": return qsTr("Allow screen recording on %1").arg(computer);
        case "failed": return failure;
        default: return "";
        }
    }
    readonly property string iconName: {
        switch (showing) {
        case "sharingOff": return "eye-off";
        case "offline": return "offline";
        case "locked": return "lock";
        case "anotherAccount": return "people";
        case "asleep": return "moon";
        case "recording": return "record";
        default: return "warning";
        }
    }
    property bool restedLong: false
    property bool waitedLong: false

    signal retried()

    onShowingChanged: {
        if (showing !== "resting")
            restedLong = false;
        if (showing !== "waiting")
            waitedLong = false;
    }

    Timer {
        interval: 6000
        running: root.showing === "resting"
        onTriggered: root.restedLong = true
    }
    Timer {
        interval: 500
        running: root.showing === "waiting"
        onTriggered: root.waitedLong = true
    }
    ColumnLayout {
        id: card

        property string line: ""
        property string iconName: ""
        property bool retries: false

        anchors.centerIn: parent
        width: Math.min(360, parent.width - 48)
        spacing: 12
        opacity: root.line.length > 0 ? 1 : 0
        visible: opacity > 0
        objectName: root.objectName + ".line"
        Accessible.role: Accessible.StaticText
        Accessible.name: card.line
        Accessible.description: card.retries ? root.problem : ""

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }

        // The card keeps its words while it fades away.
        Binding { target: card; property: "line"; value: root.line; when: root.line.length > 0; restoreMode: Binding.RestoreNone }
        Binding { target: card; property: "iconName"; value: root.iconName; when: root.line.length > 0; restoreMode: Binding.RestoreNone }
        Binding {
            target: card
            property: "retries"
            value: root.showing === "failed"
            when: root.line.length > 0
            restoreMode: Binding.RestoreNone
        }

        IconWell {
            Layout.alignment: Qt.AlignHCenter
            iconName: card.iconName
        }
        PlainLabel {
            Layout.fillWidth: true
            text: card.line
            font.pixelSize: KodosiTheme.fontHeadline
            font.weight: Font.DemiBold
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
        KButton {
            Layout.alignment: Qt.AlignHCenter
            visible: card.retries
            text: qsTr("Try again")
            objectName: root.objectName + ".retry"
            Accessible.id: objectName
            onClicked: root.retried()
        }
    }
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: 12
        width: 7
        height: 7
        radius: 3.5
        color: KodosiTheme.darkLook.glowOrange
        opacity: root.showing === "resting" && !root.restedLong ? 1 : 0
        visible: opacity > 0
        Accessible.ignored: true

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
    }
    ReconnectingNotice {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.topMargin: 10
        look: KodosiTheme.darkLook
        shown: root.showing === "resting" && root.restedLong
        objectName: root.objectName + ".reconnecting"
    }
    Spinner {
        anchors.centerIn: parent
        size: 18
        opacity: root.showing === "waiting" && root.waitedLong ? 1 : 0
        visible: opacity > 0

        Behavior on opacity { NumberAnimation { duration: KodosiTheme.motionFade } }
    }
}
