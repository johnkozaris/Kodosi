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

/// The CSS name of a cursor, which every viewer knows.
pub(crate) fn css_name(cursor: CursorType) -> &'static str {
    NAMES
        .iter()
        .find(|(known, _)| *known == cursor)
        .map_or("default", |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_has_its_css_name_and_the_arrow_is_the_default() {
        assert_eq!(css_name(CursorType::HAND), "pointer");
        assert_eq!(css_name(CursorType::IBEAM), "text");
        assert_eq!(css_name(CursorType::NOTALLOWED), "not-allowed");
        assert_eq!(css_name(CursorType::POINTER), "default");
    }
}
