pragma ComponentBehavior: Bound

import Kodosi 1.0
import QtQuick
import QtQuick.Controls
import Kodosi.Models 1.0 as Models

// The keys of a terminal's canvas, one for each of its parts: its files and its browser. The
// chosen part shows its name. When the agent shows or changes a file, the files key shows that
// file's name for a moment. On the canvas, a pointer on a key shows the key that closes the part
// on this device, and the plus key lists the parts to open.
Row {
    id: root

    required property string sessionId
    property string chosen: ""
    property bool inPanel: false
    property string identifier: "canvas.key"
    property var session: Models.Sessions.presentationForSession(sessionId)
    readonly property var canvas: session.canvas || null
    readonly property var present: canvas
        ? ((canvas.items || []).length > 0 ? ["files"] : []).concat(canvas.browser ? ["browser"] : []) : []
    readonly property var sections: Models.Canvas.revision >= 0 && inPanel ? Models.Canvas.sections(sessionId) : present
    readonly property var closed: Models.Canvas.revision >= 0 && inPanel ? Models.Canvas.closedSections(sessionId) : []
    readonly property var newest: canvas ? (canvas.items || []).reduce((best, item) => !best || item.changed > best.changed ? item : best, null)
        : null
    readonly property string change: newest ? newest.key + "\n" + newest.changed : ""
    readonly property bool browsing: !!canvas && !!canvas.browser
    property string toldSection: ""
    property string toldWords: ""
    property int flashes: 0
    property bool ready: false

    signal pressed(string section)
    signal closeRequested(string section)
    signal openRequested(string section)

    function title(section: string): string {
        return section === "browser" ? qsTr("Browser") : qsTr("Files");
    }
    function tell(section: string, words: string) {
        toldSection = section;
        toldWords = words;
        telling.restart();
    }

    spacing: 4
    onChangeChanged: {
        if (ready && newest) {
            tell("files", newest.name);
            ++flashes;
        }
    }
    onBrowsingChanged: {
        if (ready && browsing)
            tell("browser", title("browser"));
    }
    Component.onCompleted: {
        ready = true;
        if (newest && Models.Canvas.isNew(sessionId, newest.key))
            tell("files", newest.name);
    }

    Connections {
        function onModelReset() {
            root.session = Models.Sessions.presentationForSession(root.sessionId);
        }

        target: Models.Sessions
    }
    Timer {
        id: telling

        interval: 4000
        onTriggered: root.toldSection = ""
    }
    Item {
        implicitWidth: keys.implicitWidth + 4
        implicitHeight: 28

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: KodosiTheme.terminalWell
            border.width: 1
            border.color: Qt.tint(KodosiTheme.terminalWell, KodosiTheme.alpha("#000000", 0.4))
        }
        Row {
            id: keys

            x: 2
            y: 2
            spacing: 2

            Repeater {
                model: root.sections

                AbstractButton {
                    id: key

                    required property string modelData
                    readonly property bool selected: modelData === root.chosen
                    readonly property bool closing: root.inPanel && hovered
                    readonly property bool news: !selected && Models.Canvas.revision >= 0 && root.canvas
                        && ((modelData === "browser" && Models.Canvas.isNew(root.sessionId, "browser"))
                            || (modelData === "files" && (root.canvas.items || []).some(item => Models.Canvas.isNew(root.sessionId, item.key))))
                    readonly property string words: selected || closing ? root.title(modelData)
                        : root.toldSection === modelData ? root.toldWords : ""
                    readonly property int count: modelData === "files" && root.canvas ? (root.canvas.items || []).length : 0
                    readonly property bool doing: modelData === "browser" && !!root.canvas && !!root.canvas.browser
                        && !!root.canvas.browser.doing

                    height: 24
                    implicitWidth: Math.max(26, keyRow.implicitWidth + (words.length > 0 ? 20 : 14) + (closing ? 16 : 0))
                    hoverEnabled: true
                    objectName: root.identifier + "." + modelData
                    Accessible.id: objectName
                    Accessible.name: root.title(modelData)
                    Accessible.role: Accessible.PageTab
                    Accessible.selected: selected
                    ToolTip.text: root.title(modelData)
                    ToolTip.visible: hovered && words.length === 0
                    ToolTip.delay: Tip.delay

                    Behavior on implicitWidth { NumberAnimation { duration: KodosiTheme.motionSnappy; easing.type: Easing.OutCubic } }

                    contentItem: Item {
                        Row {
                            id: keyRow

                            x: key.words.length > 0 ? 9 : 7
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 5

                            KIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 13
                                height: 13
                                name: key.modelData === "browser" ? "globe" : "files"
                                strokeWidth: 2.1
                                color: key.selected ? KodosiTheme.terminalAccentStrong : KodosiTheme.terminalInkMuted
                            }
                            PlainLabel {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: key.words.length > 0
                                width: Math.min(implicitWidth, 160)
                                text: key.words
                                color: key.selected ? KodosiTheme.terminalInk : KodosiTheme.terminalInkMuted
                                font.pixelSize: KodosiTheme.fontFootnote
                                font.weight: Font.Medium
                                elide: Text.ElideMiddle
                            }
                            PlainLabel {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: key.words.length === 0 && key.count > 1
                                text: String(key.count)
                                color: KodosiTheme.terminalInkFaint
                                font.pixelSize: KodosiTheme.fontCaption
                            }
                        }
                    }
                    background: Item {
                        Cue {
                            anchors.fill: parent
                            radius: height / 2
                            glints: key.modelData === "files" ? root.flashes : 0
                        }
                        Raised {
                            anchors.fill: parent
                            radius: height / 2
                            fill: KodosiTheme.terminalLifted
                            dark: true
                            visible: key.selected
                        }
                        LightRim {
                            anchors.fill: parent
                            radius: height / 2
                            lineWidth: 1.5
                            tile: true
                            visible: key.doing
                        }
                        BreathingDot {
                            x: parent.width - 7
                            y: 1
                            size: 6
                            color: KodosiTheme.glowOrange
                            visible: key.news
                        }
                    }
                    onClicked: root.pressed(modelData)

                    AbstractButton {
                        id: close

                        z: 2
                        anchors.right: parent.right
                        anchors.rightMargin: 5
                        anchors.verticalCenter: parent.verticalCenter
                        width: 14
                        height: 14
                        visible: key.closing
                        hoverEnabled: true
                        objectName: root.identifier + ".close." + key.modelData
                        Accessible.id: objectName
                        Accessible.name: qsTr("Close")
                        ToolTip.text: qsTr("Close")
                        ToolTip.visible: hovered
                        ToolTip.delay: Tip.delay
                        contentItem: KIcon {
                            name: "close"
                            strokeWidth: 3
                            color: close.hovered ? KodosiTheme.terminalInk : KodosiTheme.terminalInkMuted
                        }
                        background: Rectangle {
                            radius: width / 2
                            color: KodosiTheme.alpha(KodosiTheme.terminalInk, close.hovered ? 0.14 : 0.08)
                        }
                        onClicked: root.closeRequested(key.modelData)
                    }
                }
            }
        }
    }
    KIconButton {
        id: adder

        anchors.verticalCenter: parent.verticalCenter
        visible: root.inPanel && root.closed.length > 0
        glyph: "plus"
        size: 26
        onTerminal: true
        open: openMenu.visible
        objectName: root.identifier + ".open"
        Accessible.id: objectName
        Accessible.name: qsTr("Open")
        onClicked: openMenu.popup(adder, 0, adder.height + 4)

        KMenu {
            id: openMenu

            Instantiator {
                model: root.closed
                onObjectAdded: (index, item) => openMenu.insertItem(index, item)
                onObjectRemoved: (index, item) => openMenu.removeItem(item)
                delegate: KMenuItem {
                    required property string modelData

                    text: root.title(modelData)
                    iconName: modelData === "browser" ? "globe" : "files"
                    objectName: root.identifier + ".open." + modelData
                    onTriggered: root.openRequested(modelData)
                }
            }
        }
    }
}
