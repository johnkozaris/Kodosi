use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use document::Unwritable;
use package::{Package, Parts, escape, rewrite};
use roxmltree::{Document, Node as Xml};

use crate::{
    calculate::calculated,
    formats,
    formula::{cell_name, column_name, parse_cell, parse_range, range_name},
    model::{Cell, Look, Model, Sheet, Value},
    read::{ANCHORS, STYLES_PART, WORKBOOK_PART, cell_of, local, locals, merges_of, widths_of},
};

const CONTENT_TYPES: &str = "[Content_Types].xml";
const WORKBOOK_RELATIONS: &str = "xl/_rels/workbook.xml.rels";
const SHEET_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const SHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const SHEET_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// What the file format puts after `mergeCells`, the first of which a new one stands before.
const AFTER_MERGES: [&str; 22] = [
    "phoneticPr",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "customProperties",
    "cellWatches",
    "ignoredErrors",
    "smartTags",
    "drawing",
    "legacyDrawing",
    "legacyDrawingHF",
    "picture",
    "oleObjects",
    "controls",
    "tableParts",
    "extLst",
];

type Edit = (std::ops::Range<usize>, String);

fn spliced(xml: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    let mut new_xml = String::with_capacity(xml.len());
    let mut at = 0;
    for (range, text) in edits {
        new_xml.push_str(&xml[at..range.start]);
        new_xml.push_str(&text);
        at = range.end;
    }
    new_xml.push_str(&xml[at..]);
    new_xml
}

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
        add_rows(xml, data, sheet, &waiting, &mut edits)?;
    }
    if edits.is_empty() {
        return Ok(None);
    }
    edits.extend(dimension_edit(xml, parsed.root_element(), sheet));
    Ok(Some(spliced(xml, edits)))
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
    sheet: &Sheet,
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
        let mut text = format!("{}>", row_tag(sheet, *row));
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

/// The edits that bring the ranges of a moved sheet's other elements up to date.
fn anchor_edits(xml: &str, root: Xml<'_, '_>, sheet: &Sheet) -> Vec<Edit> {
    let elements = root
        .descendants()
        .filter(Xml::is_element)
        .filter(|element| {
            ANCHORS.iter().any(|(tag, attribute)| {
                *tag == element.tag_name().name() && element.has_attribute(*attribute)
            })
        });
    elements
        .zip(&sheet.anchors)
        .filter_map(|(element, anchor)| {
            let old = element.attribute(anchor.attribute)?;
            if old == anchor.ranges {
                return None;
            }
            if anchor.ranges.is_empty() {
                return Some((element.range(), String::new()));
            }
            let text = &xml[element.range()];
            let new = text.replacen(
                &format!(r#"{}="{old}""#, anchor.attribute),
                &format!(r#"{}="{}""#, anchor.attribute, anchor.ranges),
                1,
            );
            Some((element.range(), new))
        })
        .collect()
}

/// Writes a whole sheet, for a sheet whose rows or columns moved: the empty end tag form of
/// `sheetData` also comes here.
fn whole_sheet(xml: &str, sheet: &Sheet) -> Result<String, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    let root = parsed.root_element();
    let data = local(root, "sheetData").ok_or("the sheet has no data")?;
    let mut edits = anchor_edits(xml, root, sheet);
    edits.push((
        data.range(),
        format!("<sheetData>{}</sheetData>", rows_xml(sheet)),
    ));
    if let Some(dimension) = local(root, "dimension") {
        let (rows, columns) = sheet.size();
        let name = format!(
            r#"<dimension ref="A1:{}{}"/>"#,
            column_name(columns.saturating_sub(1)),
            rows.max(1)
        );
        edits.push((dimension.range(), name));
    }
    Ok(spliced(xml, edits))
}

fn row_tag(sheet: &Sheet, row: u32) -> String {
    let mut tag = format!(r#"<row r="{}""#, row + 1);
    if let Some(look) = sheet.row_looks.get(&row) {
        let _ = write!(tag, " {look}");
    }
    if let Some(height) = sheet.heights.get(&row) {
        let _ = write!(tag, r#" ht="{height}" customHeight="1""#);
    }
    tag
}

fn rows_xml(sheet: &Sheet) -> String {
    let mut rows: BTreeMap<u32, String> = sheet
        .heights
        .keys()
        .chain(sheet.row_looks.keys())
        .map(|row| (*row, String::new()))
        .collect();
    for ((row, column), cell) in &sheet.cells {
        rows.entry(*row)
            .or_default()
            .push_str(&cell_xml(*row, *column, cell));
    }
    let mut text = String::new();
    for (row, cells) in rows {
        let tag = row_tag(sheet, row);
        if cells.is_empty() {
            let _ = write!(text, "{tag}/>");
        } else {
            let _ = write!(text, "{tag}>{cells}</row>");
        }
    }
    text
}

/// `cols` as the model holds it, when the file differs.
fn columns_edit(root: Xml<'_, '_>, data_start: usize, sheet: &Sheet) -> Option<Edit> {
    if widths_of(root) == sheet.widths {
        return None;
    }
    let lines: BTreeSet<u32> = sheet.widths.keys().copied().collect();
    let mut text = String::new();
    for line in lines {
        let number = line + 1;
        let _ = write!(text, r#"<col min="{number}" max="{number}""#);
        if let Some(width) = sheet.widths.get(&line) {
            let _ = write!(text, r#" width="{width}" customWidth="1""#);
        }
        if let Some(look) = sheet.column_looks.get(&line) {
            let _ = write!(text, " {look}");
        }
        text.push_str("/>");
    }
    let text = format!("<cols>{text}</cols>");
    Some(local(root, "cols").map_or_else(
        || (data_start..data_start, text.clone()),
        |columns| (columns.range(), text.clone()),
    ))
}

/// `mergeCells` as the model holds it, when the file differs.
fn merges_edit(root: Xml<'_, '_>, xml: &str, sheet: &Sheet) -> Option<Edit> {
    if merges_of(root) == sheet.merges {
        return None;
    }
    let mut cells = String::new();
    for merge in &sheet.merges {
        let _ = write!(cells, r#"<mergeCell ref="{}"/>"#, range_name(*merge));
    }
    let text = if cells.is_empty() {
        String::new()
    } else {
        format!(
            r#"<mergeCells count="{}">{cells}</mergeCells>"#,
            sheet.merges.len()
        )
    };
    if let Some(merges) = local(root, "mergeCells") {
        return Some((merges.range(), text));
    }
    let at = root
        .children()
        .filter(Xml::is_element)
        .find(|element| AFTER_MERGES.contains(&element.tag_name().name()))
        .map_or_else(
            || root.range().start + xml[root.range()].rfind("</").unwrap_or(0),
            |element| element.range().start,
        );
    Some((at..at, text))
}

fn height_edit(xml: &str, row: Xml<'_, '_>, sheet: &Sheet, line: u32) -> Option<Edit> {
    let wanted = sheet.heights.get(&line).copied();
    let stated = row.attribute("ht").and_then(|height| height.parse().ok());
    if wanted == stated {
        return None;
    }
    let start = row.range().start;
    let end = start + xml[start..].find('>')?;
    let closed = xml[..end].ends_with('/');
    let mut tag = format!("<{} ", tag_name(xml, row));
    let kept: Vec<String> = row
        .attributes()
        .filter(|attribute| !matches!(attribute.name(), "ht" | "customHeight"))
        .map(|attribute| {
            let prefix = attribute
                .namespace()
                .and_then(|space| row.lookup_prefix(space))
                .map_or_else(String::new, |prefix| format!("{prefix}:"));
            format!(
                r#"{prefix}{}="{}""#,
                attribute.name(),
                escape(attribute.value())
            )
        })
        .collect();
    tag.push_str(&kept.join(" "));
    if let Some(height) = wanted {
        let _ = write!(tag, r#" ht="{height}" customHeight="1""#);
    }
    Some((start..end - usize::from(closed), tag))
}

fn tag_name<'x>(xml: &'x str, element: Xml<'_, '_>) -> &'x str {
    xml[element.range().start + 1..]
        .split([' ', '/', '>'])
        .next()
        .unwrap_or_default()
}

/// The edits that bring the file's column widths, row heights and merges up to the model.
fn layout_edits(xml: &str, sheet: &Sheet) -> Result<Vec<Edit>, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    let root = parsed.root_element();
    let data = local(root, "sheetData").ok_or("the sheet has no data")?;
    let mut edits: Vec<Edit> = Vec::new();
    edits.extend(columns_edit(root, data.range().start, sheet));
    edits.extend(merges_edit(root, xml, sheet));
    let mut known = BTreeSet::new();
    for row in locals(data, "row") {
        let Some(line) = row
            .attribute("r")
            .and_then(|row| row.parse::<u32>().ok())
            .and_then(|row| row.checked_sub(1))
        else {
            continue;
        };
        known.insert(line);
        edits.extend(height_edit(xml, row, sheet, line));
    }
    let missing: BTreeMap<u32, BTreeMap<u32, &Cell>> = sheet
        .heights
        .keys()
        .filter(|line| !known.contains(line))
        .map(|line| (*line, BTreeMap::new()))
        .collect();
    if !missing.is_empty() {
        add_rows(xml, data, sheet, &missing, &mut edits)?;
    }
    Ok(edits)
}

/// A sheet with its changed cells swapped, and its layout as the model holds it.
fn write_sheet(xml: &str, sheet: &Sheet, strings: &[String]) -> Result<Option<String>, String> {
    let cells = if sheet.moved {
        Some(whole_sheet(xml, sheet)?)
    } else {
        patch_sheet(xml, sheet, strings)?
    };
    let current = cells.as_deref().unwrap_or(xml);
    let edits = layout_edits(current, sheet)?;
    if edits.is_empty() {
        return Ok(cells);
    }
    Ok(Some(spliced(current, edits)))
}

fn new_sheet_xml(sheet: &Sheet) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{SHEET_MAIN}"><sheetData>{}</sheetData></worksheet>"#,
        rows_xml(sheet)
    )
}

/// The XML without the children of its root that `gone` names.
fn without(xml: &str, gone: impl Fn(Xml<'_, '_>) -> bool) -> Result<String, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    let edits = parsed
        .root_element()
        .children()
        .filter(|node| node.is_element() && gone(*node))
        .map(|node| (node.range(), String::new()))
        .collect();
    Ok(spliced(xml, edits))
}

/// The XML with more children at the end of its root.
fn with_added(xml: &str, added: &str) -> Result<String, String> {
    let closing = xml.rfind("</").ok_or("the package is broken")?;
    Ok(format!("{}{added}{}", &xml[..closing], &xml[closing..]))
}

/// Takes the sheets that the model no longer has out of the package: their parts, their
/// relationships and their content types. Gives the relationships and the content types that
/// stay, and whether a sheet went.
fn removed_sheets(
    model: &Model,
    list: Xml<'_, '_>,
    package: &mut Package<'_>,
    parts: &mut Parts,
) -> Result<(String, String, bool), String> {
    let removed: Vec<&str> = locals(list, "sheet")
        .filter(|entry| {
            entry
                .attribute("sheetId")
                .and_then(|id| id.parse::<u32>().ok())
                .is_some_and(|number| model.sheets.iter().all(|sheet| sheet.number != number))
        })
        .filter_map(|entry| entry.attribute((RELATIONSHIP, "id")))
        .collect();
    let relations = package.relations(WORKBOOK_PART);
    let gone_parts: Vec<String> = removed
        .iter()
        .filter_map(|id| relations.get(*id))
        .map(|relation| relation.target.clone())
        .collect();
    for part in &gone_parts {
        let (folder, file) = part.rsplit_once('/').unwrap_or(("", part));
        let own_relations = format!("{folder}/_rels/{file}.rels");
        if package.has(&own_relations) {
            parts.insert(own_relations, None);
        }
        parts.insert(part.clone(), None);
    }
    let relations_xml = without(
        &package
            .text(WORKBOOK_RELATIONS)
            .ok_or("the workbook relationships are missing")?,
        |node| node.attribute("Id").is_some_and(|id| removed.contains(&id)),
    )?;
    let types_xml = without(
        &package
            .text(CONTENT_TYPES)
            .ok_or("the content types are missing")?,
        |node| {
            node.attribute("PartName")
                .and_then(|name| name.strip_prefix('/'))
                .is_some_and(|name| gone_parts.iter().any(|part| part == name))
        },
    )?;
    Ok((relations_xml, types_xml, !removed.is_empty()))
}

/// Puts the sheets of the model in the workbook. A new sheet gets its part, and a removed one
/// takes its part, its relationship and its content type with it.
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
    let known: Vec<(u32, String, std::ops::Range<usize>)> = locals(list, "sheet")
        .filter_map(|sheet| {
            Some((
                sheet.attribute("sheetId")?.parse().ok()?,
                sheet.attribute("name")?.to_owned(),
                sheet.range(),
            ))
        })
        .collect();
    let same = known.len() == model.sheets.len()
        && known
            .iter()
            .zip(&model.sheets)
            .all(|((number, name, _), sheet)| *number == sheet.number && *name == sheet.name);
    if same {
        return Ok(());
    }
    let (relations_xml, types_xml, removed) = removed_sheets(model, list, package, parts)?;
    let mut relations_added = String::new();
    let mut types_added = String::new();
    let mut entries = String::new();
    for sheet in &model.sheets {
        if let Some((_, name, range)) = known.iter().find(|(number, ..)| *number == sheet.number) {
            let original = &xml[range.clone()];
            if *name == sheet.name {
                entries.push_str(original);
            } else {
                let old = format!(r#"name="{}""#, escape(name));
                let new = format!(r#"name="{}""#, escape(&sheet.name));
                entries.push_str(&original.replacen(&old, &new, 1));
            }
            continue;
        }
        let number = sheet.number;
        let part = sheet.part_name();
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
    if !relations_added.is_empty() || removed {
        let relations = with_added(&relations_xml, &relations_added)?;
        parts.insert(WORKBOOK_RELATIONS.to_owned(), Some(relations.into_bytes()));
        let types = with_added(&types_xml, &types_added)?;
        parts.insert(CONTENT_TYPES.to_owned(), Some(types.into_bytes()));
    }
    Ok(())
}

/// Brings the text of the defined names up to the model.
fn names_edit(model: &Model, package: &mut Package<'_>, parts: &mut Parts) -> Result<(), String> {
    let edited = parts.get(WORKBOOK_PART).and_then(Option::as_ref);
    let xml = match edited {
        Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        None => package
            .text(WORKBOOK_PART)
            .ok_or("the workbook is missing")?,
    };
    let parsed = Document::parse(&xml).map_err(|error| error.to_string())?;
    let Some(names) = local(parsed.root_element(), "definedNames") else {
        return Ok(());
    };
    let edits: Vec<Edit> = locals(names, "definedName")
        .zip(&model.names)
        .filter(|(name, defined)| name.text().unwrap_or_default() != defined.text)
        .filter_map(|(name, defined)| {
            let node = name.first_child().filter(Xml::is_text)?;
            Some((node.range(), escape(&defined.text)))
        })
        .collect();
    if !edits.is_empty() {
        parts.insert(
            WORKBOOK_PART.to_owned(),
            Some(spliced(&xml, edits).into_bytes()),
        );
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
    if look.strike {
        xml.push_str("<strike/>");
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

/// The number formats of a file: its own ones by id, and those that a save adds.
struct NumberFormats {
    codes: Vec<(u32, String)>,
    next: u32,
    added: String,
}

impl NumberFormats {
    fn of(root: Xml<'_, '_>) -> Self {
        let codes: Vec<(u32, String)> = local(root, "numFmts").map_or_else(Vec::new, |list| {
            locals(list, "numFmt")
                .filter_map(|format| {
                    let id = format.attribute("numFmtId")?.parse().ok()?;
                    Some((id, format.attribute("formatCode")?.to_owned()))
                })
                .collect()
        });
        let next = codes
            .iter()
            .map(|(id, _)| id + 1)
            .max()
            .unwrap_or(formats::FIRST_OWN)
            .max(formats::FIRST_OWN);
        Self {
            codes,
            next,
            added: String::new(),
        }
    }

    /// The id of a format: Excel's own, the file's, or a new one at the next free id.
    fn id(&mut self, code: &str) -> u32 {
        if let Some(id) = formats::built_in_id(code) {
            return id;
        }
        if let Some((id, _)) = self.codes.iter().find(|(_, known)| known == code) {
            return *id;
        }
        let id = self.next;
        self.next += 1;
        let _ = write!(
            self.added,
            r#"<numFmt numFmtId="{id}" formatCode="{}"/>"#,
            escape(code)
        );
        self.codes.push((id, code.to_owned()));
        id
    }
}

fn cell_format_xml(look: &Look, font: usize, fill: usize, format: u32) -> String {
    let mut xml = format!(
        r#"<xf numFmtId="{format}" fontId="{font}" fillId="{fill}" borderId="0" xfId="0" applyFont="1""#
    );
    if look.align.is_none() && look.vertical.is_none() && !look.wrap {
        xml.push_str("/>");
        return xml;
    }
    xml.push_str(r#" applyAlignment="1"><alignment"#);
    if let Some(align) = &look.align {
        let _ = write!(xml, r#" horizontal="{}""#, escape(align));
    }
    if let Some(vertical) = &look.vertical {
        let _ = write!(xml, r#" vertical="{}""#, escape(vertical));
    }
    if look.wrap {
        xml.push_str(r#" wrapText="1""#);
    }
    xml.push_str("/></xf>");
    xml
}

/// Adds the looks that the file's styles lack, after the ones that it holds, so that each look
/// keeps its index.
fn add_looks(model: &Model, package: &mut Package<'_>, parts: &mut Parts) -> Result<(), String> {
    let Some(xml) = package.text(STYLES_PART) else {
        return if model.looks.is_empty() {
            Ok(())
        } else {
            Err("the file has no styles".to_owned())
        };
    };
    let parsed = Document::parse(&xml).map_err(|error| error.to_string())?;
    let root = parsed.root_element();
    let count =
        |name: &str, item: &str| local(root, name).map_or(0, |list| locals(list, item).count());
    let new_looks = &model.looks[count("cellXfs", "xf").min(model.looks.len())..];
    if new_looks.is_empty() {
        return Ok(());
    }
    let mut fonts = count("fonts", "font");
    let mut fills = count("fills", "fill");
    let mut formats = NumberFormats::of(root);
    let mut fonts_added = String::new();
    let mut fills_added = String::new();
    let mut cells_added = String::new();
    for look in new_looks {
        fonts_added.push_str(&font_xml(look));
        fonts += 1;
        let fill = look.fill.as_ref().map_or(0, |fill| {
            let _ = write!(
                fills_added,
                r#"<fill><patternFill patternType="solid"><fgColor rgb="FF{}"/></patternFill></fill>"#,
                escape(fill)
            );
            fills += 1;
            fills - 1
        });
        let format = look
            .number_format
            .as_ref()
            .map_or(0, |code| formats.id(code));
        cells_added.push_str(&cell_format_xml(look, fonts - 1, fill, format));
    }
    let formats = formats.added;
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
        let part = sheet.part_name();
        let Some(xml) = package.text(&part) else {
            continue;
        };
        if let Some(patched) = write_sheet(&xml, sheet, &strings)? {
            parts.insert(part, Some(patched.into_bytes()));
        }
    }
    workbook_edits(model, &mut package, &mut parts)?;
    names_edit(model, &mut package, &mut parts)?;
    add_looks(model, &mut package, &mut parts)?;
    if parts.is_empty() {
        return Ok(None);
    }
    rewrite(file, &parts)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Writes the file with the current result of each formula.
pub(crate) fn write(model: &Model, file: &[u8]) -> Result<Vec<u8>, Unwritable> {
    let current = calculated(model);
    Ok(write_parts(&current, file)
        .map_err(failed)?
        .unwrap_or_else(|| file.to_vec()))
}
