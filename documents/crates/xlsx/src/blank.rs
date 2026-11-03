use std::{fmt::Write as _, io::Write as _};

use package::escape;
use serde_json::Value as Json;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::formula::cell_name;

const TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="xml" ContentType="application/xml"/>"#,
    r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
    r#"<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
    r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
    r#"</Types>"#,
);
const ROOT_RELATIONS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#,
    r#"</Relationships>"#,
);
const WORKBOOK: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
    r#"<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
);
const RELATIONS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>"#,
    r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    r#"</Relationships>"#,
);
const STYLES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    r#"<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>"#,
    r#"<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>"#,
    r#"<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>"#,
    r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#,
    r#"<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>"#,
    r#"</styleSheet>"#,
);

/// Splits one line of CSV, with quotes as in RFC 4180.
fn csv_line(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut letters = line.chars().peekable();
    while let Some(letter) = letters.next() {
        match (letter, quoted) {
            ('"', true) if letters.peek() == Some(&'"') => {
                letters.next();
                if let Some(cell) = cells.last_mut() {
                    cell.push('"');
                }
            }
            ('"', _) => quoted = !quoted,
            (',', false) => cells.push(String::new()),
            (letter, _) => {
                if let Some(cell) = cells.last_mut() {
                    cell.push(letter);
                }
            }
        }
    }
    cells
}

fn rows_of_csv(text: &str) -> Vec<Vec<Json>> {
    text.lines()
        .map(|line| {
            csv_line(line)
                .into_iter()
                .map(|cell| {
                    cell.parse::<f64>()
                        .map_or_else(|_| Json::String(cell), |number| serde_json::json!(number))
                })
                .collect()
        })
        .collect()
}

/// Rows from a JSON list of lists, or from a list of objects with their keys as the first row.
fn rows_of_json(text: &str) -> Result<Vec<Vec<Json>>, String> {
    let parsed: Json = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let items = parsed.as_array().ok_or("the JSON must be a list")?;
    if items.iter().all(Json::is_array) {
        return Ok(items
            .iter()
            .filter_map(Json::as_array)
            .map(Clone::clone)
            .collect());
    }
    let mut keys: Vec<String> = Vec::new();
    for item in items {
        for key in item
            .as_object()
            .ok_or("each item must be an object")?
            .keys()
        {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }
    let mut rows = vec![keys.iter().map(|key| Json::String(key.clone())).collect()];
    rows.extend(items.iter().map(|item| {
        keys.iter()
            .map(|key| item.get(key).cloned().unwrap_or(Json::Null))
            .collect()
    }));
    Ok(rows)
}

fn cell_xml(row: u32, column: u32, value: &Json) -> String {
    let name = cell_name(row, column);
    match value {
        Json::Number(number) => format!(r#"<c r="{name}"><v>{number}</v></c>"#),
        Json::Bool(on) => format!(r#"<c r="{name}" t="b"><v>{}</v></c>"#, u8::from(*on)),
        Json::String(text) => format!(
            r#"<c r="{name}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
            escape(text)
        ),
        _ => String::new(),
    }
}

fn sheet_xml(rows: &[Vec<Json>]) -> String {
    let mut data = String::new();
    for (row, values) in (0_u32..).zip(rows) {
        let _ = write!(data, r#"<row r="{}">"#, row + 1);
        for (column, value) in (0_u32..).zip(values) {
            data.push_str(&cell_xml(row, column, value));
        }
        data.push_str("</row>");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{data}</sheetData></worksheet>"#
    )
}

/// A new .xlsx file, from CSV or JSON when there is some.
pub fn new_file(source: Option<(&str, &str)>) -> Result<Vec<u8>, String> {
    let rows = match source {
        None => Vec::new(),
        Some(("json", text)) => rows_of_json(text)?,
        Some((_, text)) => rows_of_csv(text),
    };
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let error = |error: zip::result::ZipError| error.to_string();
    for (name, bytes) in [
        ("[Content_Types].xml", TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELATIONS.to_owned()),
        ("xl/workbook.xml", WORKBOOK.to_owned()),
        ("xl/_rels/workbook.xml.rels", RELATIONS.to_owned()),
        ("xl/styles.xml", STYLES.to_owned()),
        ("xl/worksheets/sheet1.xml", sheet_xml(&rows)),
    ] {
        writer.start_file(name, options).map_err(error)?;
        writer
            .write_all(bytes.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Ok(writer.finish().map_err(error)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_keep_commas_in_a_cell() {
        assert_eq!(csv_line(r#"a,"b,c","d""e""#), ["a", "b,c", r#"d"e"#]);
    }

    #[test]
    fn objects_become_rows_under_their_keys() {
        let rows = rows_of_json(r#"[{"a":1},{"b":"x"}]"#).unwrap();
        assert_eq!(rows[0], [Json::from("a"), Json::from("b")]);
        assert_eq!(rows[2][1], "x");
    }
}
