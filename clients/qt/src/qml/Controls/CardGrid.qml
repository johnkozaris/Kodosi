import QtQuick

Flow {
    id: root

    property real minimum: 250
    property real maximum: 340
    property real gap: 12
    readonly property int columns: Math.max(1, Math.floor((width + gap) / (minimum + gap)))
    readonly property real cardWidth: Math.min(maximum, Math.floor((width - (columns - 1) * gap) / columns))

    spacing: gap
}
