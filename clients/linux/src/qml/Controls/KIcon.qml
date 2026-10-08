import Kodosi 1.0
import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property string name: ""
    property color color: KodosiTheme.textSecondary
    property real strokeWidth: 1.8

    readonly property string path1: {
        switch (name) {
        case "computer": return "M4 4 H20 V17 H4 Z M8 21 H16 M12 17 V21"
        case "plus": return "M12 5 V19 M5 12 H19"
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
        case "settings": return "M12 8.5 A3.5 3.5 0 1 0 12 15.5 A3.5 3.5 0 1 0 12 8.5 M12 3 V5 M12 19 V21 M3 12 H5 M19 12 H21 M5.64 5.64 L7.05 7.05 M16.95 16.95 L18.36 18.36 M18.36 5.64 L16.95 7.05 M7.05 16.95 L5.64 18.36"
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
        case "warning": return "M12 3 L22 20 H2 Z M12 9 V14 M12 17 V17.1"
        case "mission": return "M12 3 L20 7 V17 L12 21 L4 17 V7 Z M8 9 L12 7 L16 9 V15 L12 17 L8 15 Z"
        case "sidebar": return "M4 5 H20 V19 H4 Z M9 5 V19"
        case "document": return "M6 3 H14 L19 8 V21 H6 Z M14 3 V8 H19 M9 12 H16 M9 16 H16"
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
        layer.enabled: true
        layer.samples: 4

        ShapePath {
            strokeColor: root.color
            strokeWidth: root.strokeWidth
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: root.path1 }
        }
    }
}
