import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property string name: ""
    property color color: KodosiTheme.inkMuted
    property real strokeWidth: 1.8
    property bool filled: false

    readonly property string path1: {
        switch (name) {
        case "computer": return "M4 4 H20 V17 H4 Z M8 21 H16 M12 17 V21"
        case "plus": return "M12 5 V19 M5 12 H19"
        case "line-height": return "M11 6 H21 M11 12 H21 M11 18 H21 M5 5 V19 M3 7 L5 5 L7 7 M3 17 L5 19 L7 17"
        case "chat": return "M4 4 H20 V16 H10 L5 20 V16 H4 Z M8 8 H16 M8 12 H13"
        case "tasks": return "M3 6 L5 8 L8 4 M11 6 H21 M3 13 L5 15 L8 11 M11 13 H21 M4 20 H7 M11 20 H21"
        case "repository": return "M7 3 A2 2 0 1 0 7 7 A2 2 0 1 0 7 3 M7 7 V17 M7 17 A2 2 0 1 0 7 21 A2 2 0 1 0 7 17 M17 3 A2 2 0 1 0 17 7 A2 2 0 1 0 17 3 M17 7 V10 Q17 14 7 14"
        case "send": return "M12 20 V4 M5 11 L12 4 L19 11"
        case "agent": return "M12 3 L14.5 9.5 L21 12 L14.5 14.5 L12 21 L9.5 14.5 L3 12 L9.5 9.5 Z"
        case "more": return "M5 12 H5.1 M12 12 H12.1 M19 12 H19.1"
        case "minus": return "M5 12 H19"
        case "folder": return "M3 7.5 Q3 6 4.5 6 H9 L11 8 H19.5 Q21 8 21 9.5 V18 Q21 20 19 20 H5 Q3 20 3 18 Z"
        case "command": return "M9 7 A3 3 0 1 0 6 10 H18 A3 3 0 1 0 15 7 V17 A3 3 0 1 0 18 14 H6 A3 3 0 1 0 9 17 Z"
        case "people": return "M8.5 11 A3 3 0 1 0 8.5 5 A3 3 0 1 0 8.5 11 M15.5 10 A2.5 2.5 0 1 0 15.5 5 A2.5 2.5 0 1 0 15.5 10 M3.5 19 Q3.5 13.5 8.5 13.5 Q13.5 13.5 13.5 19 M13 13 Q20.5 12.5 20.5 18"
        case "settings": return "M4 7 H10 M14 7 H20 M12 5 A2 2 0 1 0 12 9 A2 2 0 1 0 12 5 M4 17 H7 M11 17 H20 M9 15 A2 2 0 1 0 9 19 A2 2 0 1 0 9 15"
        case "chevron-right": return "M9 5 L16 12 L9 19"
        case "chevron-left": return "M15 5 L8 12 L15 19"
        case "chevron-down": return "M5 9 L12 16 L19 9"
        case "chevron-up": return "M5 15 L12 8 L19 15"
        case "close": return "M6 6 L18 18 M18 6 L6 18"
        case "terminal": return "M4 5 H20 V19 H4 Z M7 9 L10 12 L7 15 M12 15 H17"
        case "sessions": return "M6 5 H20 V17 H6 Z M3 8 V20 H17"
        case "refresh": return "M19 8 A8 8 0 1 0 20 14 M19 4 V8 H15"
        case "focus": return "M4 9 V4 H9 M15 4 H20 V9 M20 15 V20 H15 M9 20 H4 V15"
        case "grid": return "M4 4 H10 V10 H4 Z M14 4 H20 V10 H14 Z M4 14 H10 V20 H4 Z M14 14 H20 V20 H14 Z"
        case "check": return "M5 12.5 L10 17 L19 7"
        case "hand": return "M8 13 V6.5 M11 12 V4.5 M14 12 V5 M17 13 V7.5 M8 13 V15 Q8 20 12.5 20 Q17 20 17 15 V13 M8 15.5 L5 12.5"
        case "question": return "M8.5 9 Q8.5 5 12 5 Q15.5 5 15.5 8.5 Q15.5 11 12 12.5 V14.5 M12 18.5 V18.6"
        case "key": return "M12 4 A3.5 3.5 0 1 0 12 11 A3.5 3.5 0 1 0 12 4 M12 11 V20 M12 16 H15 M12 19.5 H14.5"
        case "warning": return "M12 3 L22 20 H2 Z M12 9 V14 M12 17 V17.1"
        case "mission": return "M12 3 L20 7 V17 L12 21 L4 17 V7 Z M8 9 L12 7 L16 9 V15 L12 17 L8 15 Z"
        case "sidebar": return "M4 5 H20 V19 H4 Z M9 5 V19"
        case "document": return "M6 3 H14 L19 8 V21 H6 Z M14 3 V8 H19 M9 12 H16 M9 16 H16"
        case "search": return "M10.5 4 A6.5 6.5 0 1 0 10.5 17 A6.5 6.5 0 1 0 10.5 4 M15.5 15.5 L20 20"
        case "at": return "M16 12 A4 4 0 1 0 8 12 A4 4 0 1 0 16 12 M16 8 V13.5 A2.5 2.5 0 0 0 21 13.5 V12 A9 9 0 1 0 17.5 19.2"
        case "lock": return "M6 11 H18 V20 H6 Z M8.5 11 V8 A3.5 3.5 0 0 1 15.5 8 V11"
        case "history": return "M4 12 A8 8 0 1 0 6.5 6.2 M4 4 V8 H8 M12 8 V12 L15 14"
        case "copy": return "M9 9 H19 V20 H9 Z M5 15 V4 H15"
        case "share": return "M10 11 A3.5 3.5 0 1 0 10 4 A3.5 3.5 0 1 0 10 11 M3.5 20 Q3.5 14 10 14 Q13 14 14.6 15.4 M18 14 V20 M15 17 H21"
        case "arrow-right": return "M5 12 H19 M13 6 L19 12 L13 18"
        case "pencil": return "M4 20 L5 15.5 L16 4.5 L19.5 8 L8.5 19 Z M14 6.5 L17.5 10"
        case "play": return "M8 5 L19 12 L8 19 Z"
        case "link": return "M10 14 A4 4 0 0 0 15.7 14.3 L19 11 A4 4 0 0 0 13.3 5.3 L12 6.6 M14 10 A4 4 0 0 0 8.3 9.7 L5 13 A4 4 0 0 0 10.7 18.7 L12 17.4"
        case "laptop": return "M5 6 H19 V16 H5 Z M2.5 19 H21.5"
        case "circle": return "M12 4 A8 8 0 1 0 12 20 A8 8 0 1 0 12 4"
        case "info": return "M12 4 A8 8 0 1 0 12 20 A8 8 0 1 0 12 4 M12 11 V16 M12 8 V8.1"
        case "prompt": return "M8 6 L14 12 L8 18"
        case "moon": return "M20 14.5 A8 8 0 1 1 9.5 4 A6.5 6.5 0 0 0 20 14.5 Z"
        case "sun": return "M12 8 A4 4 0 1 0 12 16 A4 4 0 1 0 12 8 M12 2.5 V5 M12 19 V21.5 M2.5 12 H5 M19 12 H21.5 M5.3 5.3 L7 7 M17 17 L18.7 18.7 M18.7 5.3 L17 7 M7 17 L5.3 18.7"
        default: return ""
        }
    }

    implicitWidth: 18
    implicitHeight: 18

    Shape {
        width: 24
        height: 24
        anchors.centerIn: parent
        scale: Math.min(root.width, root.height) / 24
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            strokeColor: root.color
            strokeWidth: root.strokeWidth
            fillColor: root.filled ? root.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: root.path1 }
        }
    }
}
