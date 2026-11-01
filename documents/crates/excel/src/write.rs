use std::{collections::BTreeMap, fmt::Write as _};

use document::Unwritable;
use formualizer::workbook::{XlsxRecalculateOptions, recalculate_xlsx_bytes};
use package::{Package, Parts, escape, rewrite};
use roxmltree::{Document, Node as Xml};

use crate::{
    formula::{cell_name, column_name, parse_cell, parse_range},
    model::{Cell, Look, Model, Sheet, Value},
    read::{STYLES_PART, WORKBOOK_PART, cell_of, local, locals},
};

const CONTENT_TYPES: &str = "[Content_Types].xml";
const WORKBOOK_RELATIONS: &str = "xl/_rels/workbook.xml.rels";
const SHEET_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const SHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const SHEET_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const FIRST_FORMAT_ID: u32 = 200;

fn failed(error: impl std::fmt::Display) -> Unwritable {
    Unwritable(error.to_string())
}

fn value_xml(value: Option<&Value>, formula: Option<&str>) -> (Option<&'static str>, String) {
    let formula = formula.map_or_else(String::new, |formula| {
        format!("<f>{}</f>", escape(formula.trim_start_matches('=')))
    });
    match value {
        None => (None, formula),
        Some(Value::Number(number)) => (None, format!("{formula}<v>{number}</v>")),
        Some(Value::Bool(on)) => (Some("b"), format!("{formula}<v>{}</v>", u8::from(*on))),
        Some(Value::Error { error }) => (Some("e"), format!("{formula}<v>{}</v>", escape(error))),
        Some(Value::Text(text)) if formula.is_empty() => (
            Some("inlineStr"),
            format!(r#"<is><t xml:space="preserve">{}</t></is>"#, escape(text)),
        ),
        Some(Value::Text(text)) => (Some("str"), format!("{formula}<v>{}</v>", escape(text))),
    }
}

fn cell_xml(row: u32, column: u32, cell: &Cell) -> String {
    let (kind, inner) = value_xml(cell.value.as_ref(), cell.formula.as_deref());
    let mut xml = format!(r#"<c r="{}""#, cell_name(row, column));
    if let Some(style) = cell.style {
        let _ = write!(xml, r#" s="{style}""#);
    }
    if let Some(kind) = kind {
        let _ = write!(xml, r#" t="{kind}""#);
    }
    if inner.is_empty() {
        xml.push_str("/>");
    } else {
        let _ = write!(xml, ">{inner}</c>");
    }
    xml
}

/// The shared strings of the file, which a cell needs to compare itself with the model.
fn strings_of(package: &mut Package<'_>) -> Vec<String> {
    package
        .text("xl/sharedStrings.xml")
        .and_then(|text| {
            let parsed = Document::parse(&text).ok()?;
            Some(
                locals(parsed.root_element(), "si")
                    .map(|item| {
                        item.descendants()
                            .filter(|node| node.is_element() && node.tag_name().name() == "t")
                            .filter(|node| {
                                node.parent()
                                    .is_none_or(|parent| parent.tag_name().name() != "rPh")
                            })
                            .filter_map(|node| node.text())
                            .collect()
                    })
                    .collect(),
            )
        })
        .unwrap_or_default()
}

fn same_cell(file: &Cell, model: Option<&Cell>, element: Xml<'_, '_>) -> bool {
    let Some(model) = model else {
        return false;
    };
    let has_formula = local(element, "f").is_some();
    file.style == model.style
        && has_formula == model.formula.is_some()
        && (has_formula || file.value == model.value)
        && (!has_formula || formula_text_same(element, model))
}

/// A shared formula holds no text, so only a formula with its own text can differ from the
/// model here. A shared one that the model changed is new text in the model.
fn formula_text_same(element: Xml<'_, '_>, model: &Cell) -> bool {
    let Some(formula) = local(element, "f") else {
        return false;
    };
    if formula.attribute("t") == Some("shared") && formula.text().is_none() {
        return true;
    }
    formula.text() == model.formula.as_deref()
}

/// Swaps the cells that differ from the model, and copies all other bytes.
fn patch_sheet(xml: &str, sheet: &Sheet, strings: &[String]) -> Result<Option<String>, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    let data = local(parsed.root_element(), "sheetData").ok_or("the sheet has no data")?;
    let mut waiting: BTreeMap<u32, BTreeMap<u32, &Cell>> = BTreeMap::new();
    for ((row, column), cell) in &sheet.cells {
        waiting.entry(*row).or_default().insert(*column, cell);
    }
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for row_element in locals(data, "row") {
        let Some(row) = row_element
            .attribute("r")
            .and_then(|row| row.parse::<u32>().ok())
            .and_then(|row| row.checked_sub(1))
        else {
            continue;
        };
        let mut wanted = waiting.remove(&row).unwrap_or_default();
        let mut last_end = None;
        for element in locals(row_element, "c") {
            let Some((_, column)) = element.attribute("r").and_then(parse_cell) else {
                continue;
            };
            let before: Vec<u32> = wanted.range(..column).map(|(column, _)| *column).collect();
            let mut added = String::new();
            for earlier in before {
                if let Some(cell) = wanted.remove(&earlier) {
                    added.push_str(&cell_xml(row, earlier, cell));
                }
            }
            if !added.is_empty() {
                edits.push((element.range().start..element.range().start, added));
            }
            let model = wanted.remove(&column);
            let file = cell_of(element, strings);
            if !same_cell(&file, model, element) {
                let new = model.map_or_else(String::new, |cell| cell_xml(row, column, cell));
                edits.push((element.range(), new));
            }
            last_end = Some(element.range().end);
        }
        if !wanted.is_empty() {
            let added: String = wanted
                .iter()
                .map(|(column, cell)| cell_xml(row, *column, cell))
                .collect();
            edits.push(close_row_edit(xml, row_element, last_end, added));
        }
    }
    if !waiting.is_empty() {
        add_rows(xml, data, &waiting, &mut edits)?;
    }
    if edits.is_empty() {
        return Ok(None);
    }
    edits.extend(dimension_edit(xml, parsed.root_element(), sheet));
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    let mut new_xml = String::with_capacity(xml.len());
    let mut at = 0;
    for (range, text) in edits {
        new_xml.push_str(&xml[at..range.start]);
        new_xml.push_str(&text);
        at = range.end;
    }
    new_xml.push_str(&xml[at..]);
    Ok(Some(new_xml))
}

/// A cell outside the size that a sheet states makes the file wrong, so the size grows with it.
fn dimension_edit(
    xml: &str,
    root: Xml<'_, '_>,
    sheet: &Sheet,
) -> Option<(std::ops::Range<usize>, String)> {
    let dimension = local(root, "dimension")?;
    let stated = dimension.attribute("ref")?;
    let [top, left, bottom, right] = parse_range(stated)?;
    let (rows, columns) = sheet.size();
    if rows <= bottom + 1 && columns <= right + 1 {
        return None;
    }
    let grown = format!(
        "{}:{}",
        cell_name(top, left),
        cell_name(rows.max(bottom + 1) - 1, columns.max(right + 1) - 1)
    );
    let text = xml[dimension.range()].replacen(stated, &grown, 1);
    Some((dimension.range(), text))
}

/// The edit that adds cells at the end of a row, which can be a tag with no end tag.
fn close_row_edit(
    xml: &str,
    row: Xml<'_, '_>,
    last_end: Option<usize>,
    added: String,
) -> (std::ops::Range<usize>, String) {
    if let Some(end) = last_end {
        return (end..end, added);
    }
    let range = row.range();
    let text = &xml[range.clone()];
    if let Some(open) = text.strip_suffix("/>") {
        let name = row.tag_name().name();
        let tag = text
            .get(1..)
            .and_then(|rest| rest.split([' ', '/', '>']).next())
            .unwrap_or(name);
        (range, format!("{open}>{added}</{tag}>"))
    } else {
        let closing = range.start + text.rfind("</").unwrap_or(text.len());
        (closing..closing, added)
    }
}

fn add_rows(
    xml: &str,
    data: Xml<'_, '_>,
    waiting: &BTreeMap<u32, BTreeMap<u32, &Cell>>,
    edits: &mut Vec<(std::ops::Range<usize>, String)>,
) -> Result<(), String> {
    let rows: Vec<(u32, Xml<'_, '_>)> = locals(data, "row")
        .filter_map(|row| {
            let number = row.attribute("r")?.parse::<u32>().ok()?.checked_sub(1)?;
            Some((number, row))
        })
        .collect();
    for (row, cells) in waiting {
        let mut text = format!(r#"<row r="{}">"#, row + 1);
        for (column, cell) in cells {
            text.push_str(&cell_xml(*row, *column, cell));
        }
        text.push_str("</row>");
        let next = rows.iter().find(|(number, _)| number > row);
        let at = match next {
            Some((_, element)) => element.range().start,
            None => end_of_rows(xml, data, &rows)?,
        };
        edits.push((at..at, text));
    }
    Ok(())
}

fn end_of_rows(xml: &str, data: Xml<'_, '_>, rows: &[(u32, Xml<'_, '_>)]) -> Result<usize, String> {
    if let Some((_, last)) = rows.last() {
        return Ok(last.range().end);
    }
    let text = &xml[data.range()];
    if text.ends_with("/>") {
        return Err("the sheet data has no end tag".to_owned());
    }
    Ok(data.range().start + text.rfind("</").unwrap_or(text.len()))
}

/// Writes a whole sheet, for a sheet whose rows or columns moved: the empty end tag form of
/// `sheetData` also comes here.
fn whole_sheet(xml: &str, sheet: &Sheet) -> Result<String, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    let root = parsed.root_element();
    let data = local(root, "sheetData").ok_or("the sheet has no data")?;
    let mut text = String::from("<sheetData>");
    text.push_str(&rows_xml(sheet));
    text.push_str("</sheetData>");
    let mut new_xml = format!(
        "{}{text}{}",
        &xml[..data.range().start],
        &xml[data.range().end..]
    );
    if let Some(dimension) = local(root, "dimension") {
        let (rows, columns) = sheet.size();
        let name = format!(
            r#"<dimension ref="A1:{}{}"/>"#,
            column_name(columns.saturating_sub(1)),
            rows.max(1)
        );
        new_xml = new_xml.replacen(&xml[dimension.range()], &name, 1);
    }
    Ok(new_xml)
}

fn rows_xml(sheet: &Sheet) -> String {
    let mut rows: BTreeMap<u32, String> = BTreeMap::new();
    for ((row, column), cell) in &sheet.cells {
        rows.entry(*row)
            .or_default()
            .push_str(&cell_xml(*row, *column, cell));
    }
    let mut text = String::new();
    for (row, cells) in rows {
        let _ = write!(text, r#"<row r="{}">{cells}</row>"#, row + 1);
    }
    text
}

fn new_sheet_xml(sheet: &Sheet) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{SHEET_MAIN}"><sheetData>{}</sheetData></worksheet>"#,
        rows_xml(sheet)
    )
}

fn workbook_edits(
    model: &Model,
    package: &mut Package<'_>,
    parts: &mut Parts,
) -> Result<(), String> {
    let xml = package
        .text(WORKBOOK_PART)
        .ok_or("the workbook is missing")?;
    let parsed = Document::parse(&xml).map_err(|error| error.to_string())?;
    let list = local(parsed.root_element(), "sheets").ok_or("the workbook has no sheets")?;
    let known: Vec<(String, String, std::ops::Range<usize>)> = locals(list, "sheet")
        .filter_map(|sheet| {
            Some((
                sheet.attribute("sheetId")?.to_owned(),
                sheet.attribute("name")?.to_owned(),
                sheet.range(),
            ))
        })
        .collect();
    let same = known.len() == model.sheets.len()
        && known
            .iter()
            .zip(&model.sheets)
            .all(|((id, name, _), sheet)| *id == sheet.id && *name == sheet.name);
    if same {
        return Ok(());
    }
    let relations_xml = package
        .text(WORKBOOK_RELATIONS)
        .ok_or("the workbook relationships are missing")?;
    let types_xml = package
        .text(CONTENT_TYPES)
        .ok_or("the content types are missing")?;
    let mut relations_added = String::new();
    let mut types_added = String::new();
    let mut entries = String::new();
    for sheet in &model.sheets {
        if let Some((_, _, range)) = known.iter().find(|(id, _, _)| *id == sheet.id) {
            let original = &xml[range.clone()];
            let parsed_entry = known
                .iter()
                .find(|(id, _, _)| *id == sheet.id)
                .map(|(_, name, _)| name.as_str());
            if parsed_entry == Some(sheet.name.as_str()) {
                entries.push_str(original);
            } else {
                let old = format!(r#"name="{}""#, escape(parsed_entry.unwrap_or_default()));
                let new = format!(r#"name="{}""#, escape(&sheet.name));
                entries.push_str(&original.replacen(&old, &new, 1));
            }
            continue;
        }
        let number: u32 = sheet.id.parse().map_err(|_| "a sheet has no number")?;
        let part = format!("xl/worksheets/sheetN{number}.xml");
        let relation = format!("rIdSheet{number}");
        parts.insert(part.clone(), Some(new_sheet_xml(sheet).into_bytes()));
        let _ = write!(
            relations_added,
            r#"<Relationship Id="{relation}" Type="{SHEET_KIND}" Target="worksheets/sheetN{number}.xml"/>"#
        );
        let _ = write!(
            types_added,
            r#"<Override PartName="/{part}" ContentType="{SHEET_TYPE}"/>"#
        );
        let _ = write!(
            entries,
            r#"<sheet xmlns:r="{RELATIONSHIP}" name="{}" sheetId="{number}" r:id="{relation}"/>"#,
            escape(&sheet.name)
        );
    }
    let start = known
        .first()
        .map_or_else(|| list.range().end, |(_, _, range)| range.start);
    let end = known
        .last()
        .map_or_else(|| list.range().end, |(_, _, range)| range.end);
    parts.insert(
        WORKBOOK_PART.to_owned(),
        Some(format!("{}{entries}{}", &xml[..start], &xml[end..]).into_bytes()),
    );
    if !relations_added.is_empty() {
        let closing = relations_xml
            .rfind("</")
            .ok_or("the relationships are broken")?;
        let relations = format!(
            "{}{relations_added}{}",
            &relations_xml[..closing],
            &relations_xml[closing..]
        );
        parts.insert(WORKBOOK_RELATIONS.to_owned(), Some(relations.into_bytes()));
        let closing = types_xml
            .rfind("</")
            .ok_or("the content types are broken")?;
        let types = format!(
            "{}{types_added}{}",
            &types_xml[..closing],
            &types_xml[closing..]
        );
        parts.insert(CONTENT_TYPES.to_owned(), Some(types.into_bytes()));
    }
    Ok(())
}

fn font_xml(look: &Look) -> String {
    let mut xml = String::from("<font>");
    if look.bold {
        xml.push_str("<b/>");
    }
    if look.italic {
        xml.push_str("<i/>");
    }
    if look.underline {
        xml.push_str("<u/>");
    }
    let _ = write!(xml, r#"<sz val="{}"/>"#, look.size.unwrap_or(11.0));
    if let Some(color) = &look.color {
        let _ = write!(xml, r#"<color rgb="FF{}"/>"#, escape(color));
    }
    let _ = write!(
        xml,
        r#"<name val="{}"/></font>"#,
        escape(look.font.as_deref().unwrap_or("Calibri"))
    );
    xml
}

/// Adds the new looks to the style table. Looks that the file had stay as they are.
fn add_looks(model: &Model, package: &mut Package<'_>, parts: &mut Parts) -> Result<(), String> {
    let new_looks = &model.looks[model.file_looks.min(model.looks.len())..];
    if new_looks.is_empty() {
        return Ok(());
    }
    let xml = package.text(STYLES_PART).ok_or("the file has no styles")?;
    let parsed = Document::parse(&xml).map_err(|error| error.to_string())?;
    let root = parsed.root_element();
    let count =
        |name: &str, item: &str| local(root, name).map_or(0, |list| locals(list, item).count());
    let mut fonts = count("fonts", "font");
    let mut fills = count("fills", "fill");
    let mut formats = String::new();
    let mut fonts_added = String::new();
    let mut fills_added = String::new();
    let mut cells_added = String::new();
    for (index, look) in new_looks.iter().enumerate() {
        fonts_added.push_str(&font_xml(look));
        fonts += 1;
        let fill_id = look.fill.as_ref().map_or(0, |fill| {
            let _ = write!(
                fills_added,
                r#"<fill><patternFill patternType="solid"><fgColor rgb="FF{}"/></patternFill></fill>"#,
                escape(fill)
            );
            fills += 1;
            fills - 1
        });
        let format_id = look.number_format.as_ref().map_or(0, |code| {
            let id = FIRST_FORMAT_ID + u32::try_from(index).unwrap_or(0);
            let _ = write!(
                formats,
                r#"<numFmt numFmtId="{id}" formatCode="{}"/>"#,
                escape(code)
            );
            id
        });
        let _ = write!(
            cells_added,
            r#"<xf numFmtId="{format_id}" fontId="{}" fillId="{fill_id}" borderId="0" xfId="0" applyFont="1""#,
            fonts - 1
        );
        match (&look.align, look.wrap) {
            (None, false) => cells_added.push_str("/>"),
            (align, wrap) => {
                cells_added.push_str(r#" applyAlignment="1"><alignment"#);
                if let Some(align) = align {
                    let _ = write!(cells_added, r#" horizontal="{}""#, escape(align));
                }
                if wrap {
                    cells_added.push_str(r#" wrapText="1""#);
                }
                cells_added.push_str("/></xf>");
            }
        }
    }
    let mut edits: Vec<(usize, String)> = Vec::new();
    for (name, added) in [
        ("fonts", &fonts_added),
        ("fills", &fills_added),
        ("cellXfs", &cells_added),
    ] {
        if added.is_empty() {
            continue;
        }
        let list = local(root, name).ok_or_else(|| format!("the styles have no {name}"))?;
        let text = &xml[list.range()];
        let closing = list.range().start + text.rfind("</").ok_or("the styles are broken")?;
        edits.push((closing, added.clone()));
    }
    if !formats.is_empty() {
        if let Some(list) = local(root, "numFmts") {
            let text = &xml[list.range()];
            let closing = list.range().start + text.rfind("</").ok_or("the styles are broken")?;
            edits.push((closing, formats));
        } else {
            let at = local(root, "fonts").map_or(0, |fonts| fonts.range().start);
            edits.push((at, format!("<numFmts>{formats}</numFmts>")));
        }
    }
    edits.sort_by_key(|(at, _)| *at);
    let mut new_xml = xml.clone();
    for (at, text) in edits.into_iter().rev() {
        new_xml.insert_str(at, &text);
    }
    new_xml = strip_counts(&new_xml)?;
    parts.insert(STYLES_PART.to_owned(), Some(new_xml.into_bytes()));
    Ok(())
}

/// Removes the `count` attributes of the style lists, which no longer fit and which are
/// optional.
fn strip_counts(xml: &str) -> Result<String, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    let mut edits = Vec::new();
    for name in ["numFmts", "fonts", "fills", "cellXfs"] {
        let Some(list) = local(parsed.root_element(), name) else {
            continue;
        };
        let start = list.range().start;
        let tag_end = xml[start..].find('>').map_or(start, |end| start + end);
        let tag = &xml[start..tag_end];
        if let Some(found) = tag.find(" count=\"")
            && let Some(close) = tag[found + 8..].find('"')
        {
            edits.push(start + found..start + found + 8 + close + 1);
        }
    }
    let mut new_xml = xml.to_owned();
    for range in edits.into_iter().rev() {
        new_xml.replace_range(range, "");
    }
    Ok(new_xml)
}

fn write_parts(model: &Model, file: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut package = Package::open(file).map_err(|error| error.to_string())?;
    let strings = strings_of(&mut package);
    let mut parts = Parts::new();
    for sheet in &model.sheets {
        let Some(part) = &sheet.part else {
            continue;
        };
        let Some(xml) = package.text(part) else {
            continue;
        };
        let patched = if sheet.moved {
            Some(whole_sheet(&xml, sheet)?)
        } else {
            patch_sheet(&xml, sheet, &strings)?
        };
        if let Some(patched) = patched {
            parts.insert(part.clone(), Some(patched.into_bytes()));
        }
    }
    workbook_edits(model, &mut package, &mut parts)?;
    add_looks(model, &mut package, &mut parts)?;
    if parts.is_empty() {
        return Ok(None);
    }
    rewrite(file, &parts)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Writes the file. The new formula results come from a recalculation that keeps each other
/// byte. If it has no result, the file keeps the values that it holds.
pub(crate) fn write(model: &Model, file: &[u8]) -> Result<Vec<u8>, Unwritable> {
    let Some(patched) = write_parts(model, file).map_err(failed)? else {
        return Ok(file.to_vec());
    };
    Ok(recalculated(&patched).unwrap_or(patched))
}

pub(crate) fn recalculated(file: &[u8]) -> Option<Vec<u8>> {
    recalculate_xlsx_bytes(file, XlsxRecalculateOptions::default())
        .ok()
        .map(|result| result.bytes)
}
