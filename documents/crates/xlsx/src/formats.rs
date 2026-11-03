/// The built-in number formats of .xlsx, which a file names by their id alone.
const BUILT_IN: [(u32, &str); 27] = [
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "mm-dd-yy"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "m/d/yy h:mm"),
    (37, "#,##0 ;(#,##0)"),
    (38, "#,##0 ;[Red](#,##0)"),
    (39, "#,##0.00;(#,##0.00)"),
    (40, "#,##0.00;[Red](#,##0.00)"),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mmss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

/// The first id that a file's own formats can have.
pub(crate) const FIRST_OWN: u32 = 164;

pub(crate) fn built_in_code(id: u32) -> Option<&'static str> {
    BUILT_IN
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, code)| *code)
}

pub(crate) fn built_in_id(code: &str) -> Option<u32> {
    BUILT_IN
        .iter()
        .find(|(_, known)| *known == code)
        .map(|(id, _)| *id)
}
