use cef::CursorType;

const NAMES: &[(CursorType, &str)] = &[
    (CursorType::HAND, "pointer"),
    (CursorType::IBEAM, "text"),
    (CursorType::VERTICALTEXT, "vertical-text"),
    (CursorType::CROSS, "crosshair"),
    (CursorType::WAIT, "wait"),
    (CursorType::PROGRESS, "progress"),
    (CursorType::HELP, "help"),
    (CursorType::MOVE, "move"),
    (CursorType::GRAB, "grab"),
    (CursorType::GRABBING, "grabbing"),
    (CursorType::NOTALLOWED, "not-allowed"),
    (CursorType::NODROP, "not-allowed"),
    (CursorType::COLUMNRESIZE, "col-resize"),
    (CursorType::ROWRESIZE, "row-resize"),
    (CursorType::EASTRESIZE, "e-resize"),
    (CursorType::WESTRESIZE, "w-resize"),
    (CursorType::NORTHRESIZE, "n-resize"),
    (CursorType::SOUTHRESIZE, "s-resize"),
    (CursorType::EASTWESTRESIZE, "ew-resize"),
    (CursorType::NORTHSOUTHRESIZE, "ns-resize"),
    (CursorType::NORTHEASTRESIZE, "ne-resize"),
    (CursorType::NORTHWESTRESIZE, "nw-resize"),
    (CursorType::SOUTHEASTRESIZE, "se-resize"),
    (CursorType::SOUTHWESTRESIZE, "sw-resize"),
    (CursorType::NORTHEASTSOUTHWESTRESIZE, "nesw-resize"),
    (CursorType::NORTHWESTSOUTHEASTRESIZE, "nwse-resize"),
    (CursorType::CELL, "cell"),
    (CursorType::CONTEXTMENU, "context-menu"),
    (CursorType::ALIAS, "alias"),
    (CursorType::COPY, "copy"),
    (CursorType::NONE, "none"),
    (CursorType::ZOOMIN, "zoom-in"),
    (CursorType::ZOOMOUT, "zoom-out"),
];

pub(crate) fn css_name(cursor: CursorType) -> &'static str {
    NAMES
        .iter()
        .find(|(known, _)| *known == cursor)
        .map_or("default", |(_, name)| name)
}
