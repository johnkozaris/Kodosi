use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use document::Unreadable;
use package::Package;
use roxmltree::{Document, Node as Xml};

use crate::{
    formula::{parse_cell, parse_range, translate_shared},
    model::{Cell, Look, Model, Sheet, Value},
};

pub(crate) const WORKBOOK_PART: &str = "xl/workbook.xml";
pub(crate) const STYLES_PART: &str = "xl/styles.xml";
const STRINGS_PART: &str = "xl/sharedStrings.xml";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_RELATIONSHIPS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

fn broken(error: impl std::fmt::Display) -> Unreadable {
    Unreadable(error.to_string())
}

pub(crate) fn local<'a, 'input>(node: Xml<'a, 'input>, name: &str) -> Option<Xml<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

pub(crate) fn locals<'a, 'input: 'a>(
    node: Xml<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Xml<'a, 'input>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

pub(crate) fn read(file: &[u8]) -> Result<Model, Unreadable> {
    let mut package = Package::open(file).map_err(broken)?;
    let workbook_xml = package
        .text(WORKBOOK_PART)
        .ok_or_else(|| Unreadable("the file has no workbook".to_owned()))?;
    let relations = package.relations(WORKBOOK_PART);
    let strings = package
        .text(STRINGS_PART)
        .map(|text| shared_strings(&text))
        .transpose()?
        .unwrap_or_default();
    let workbook = Document::parse(&workbook_xml).map_err(broken)?;
    let list = local(workbook.root_element(), "sheets")
        .ok_or_else(|| Unreadable("the workbook has no sheets".to_owned()))?;
    let mut sheets = Vec::new();
    for entry in locals(list, "sheet") {
        let relation = entry
            .attributes()
            .find(|attribute| {
                attribute.name() == "id"
                    && matches!(
                        attribute.namespace(),
                        Some(RELATIONSHIPS | STRICT_RELATIONSHIPS)
                    )
            })
            .map(|attribute| attribute.value());
        let part = relation
            .and_then(|id| relations.get(id))
            .map(|relation| relation.target.clone());
        let Some(part) = part else {
            continue;
        };
        let Some(xml) = package.text(&part) else {
            continue;
        };
        let mut sheet = read_sheet(&xml, &strings)?;
        entry
            .attribute("sheetId")
            .unwrap_or_default()
            .clone_into(&mut sheet.id);
        entry
            .attribute("name")
            .unwrap_or_default()
            .clone_into(&mut sheet.name);
        sheet.part = Some(part);
        sheets.push(sheet);
    }
    let looks = package
        .text(STYLES_PART)
        .map(|text| read_looks(&text))
        .transpose()?
        .unwrap_or_default();
    Ok(Model {
        sheets,
        file_looks: looks.len(),
        looks,
        base: Some(Arc::new(file.to_vec())),
    })
}

fn shared_strings(text: &str) -> Result<Vec<String>, Unreadable> {
    let parsed = Document::parse(text).map_err(broken)?;
    Ok(locals(parsed.root_element(), "si")
        .map(|item| string_item(item))
        .collect())
}

fn string_item(item: Xml<'_, '_>) -> String {
    item.descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "t")
        .filter(|node| {
            node.parent()
                .is_none_or(|parent| parent.tag_name().name() != "rPh")
        })
        .filter_map(|node| node.text())
        .collect()
}

pub(crate) fn cell_of(element: Xml<'_, '_>, strings: &[String]) -> Cell {
    let kind = element.attribute("t").unwrap_or("n");
    let text = local(element, "v").and_then(|value| value.text());
    let value = match kind {
        "s" => text
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| strings.get(index))
            .map(|text| Value::Text(text.clone())),
        "str" => text.map(|text| Value::Text(text.to_owned())),
        "inlineStr" => local(element, "is").map(|item| Value::Text(string_item(item))),
        "b" => text.map(|text| Value::Bool(text == "1")),
        "e" => text.map(|text| Value::Error {
            error: text.to_owned(),
        }),
        _ => text
            .and_then(|text| text.parse::<f64>().ok())
            .map(Value::Number),
    };
    Cell {
        value,
        formula: None,
        style: element.attribute("s").and_then(|style| style.parse().ok()),
    }
}

fn read_sheet(xml: &str, strings: &[String]) -> Result<Sheet, Unreadable> {
    let parsed = Document::parse(xml).map_err(broken)?;
    let root = parsed.root_element();
    let mut sheet = Sheet::default();
    if let Some(data) = local(root, "sheetData") {
        let mut shared: HashMap<String, (String, u32, u32)> = HashMap::new();
        for row in locals(data, "row") {
            if let Some(height) = row.attribute("ht").and_then(|height| height.parse().ok())
                && let Some(number) = row.attribute("r").and_then(|row| row.parse::<u32>().ok())
                && let Some(row) = number.checked_sub(1)
            {
                sheet.heights.insert(row, height);
            }
            for element in locals(row, "c") {
                let Some((row, column)) = element.attribute("r").and_then(parse_cell) else {
                    continue;
                };
                let mut cell = cell_of(element, strings);
                if let Some(formula) = local(element, "f") {
                    cell.formula = formula_of(formula, row, column, &mut shared)
                        .filter(|text| !text.is_empty());
                }
                if !cell.is_empty() {
                    sheet.cells.insert((row, column), cell);
                }
            }
        }
    }
    if let Some(merges) = local(root, "mergeCells") {
        sheet.merges = locals(merges, "mergeCell")
            .filter_map(|merge| parse_range(merge.attribute("ref")?))
            .collect();
    }
    if let Some(columns) = local(root, "cols") {
        for column in locals(columns, "col") {
            let (Some(first), Some(last), Some(width)) = (
                column
                    .attribute("min")
                    .and_then(|value| value.parse::<u32>().ok()),
                column
                    .attribute("max")
                    .and_then(|value| value.parse::<u32>().ok()),
                column
                    .attribute("width")
                    .and_then(|value| value.parse::<f64>().ok()),
            ) else {
                continue;
            };
            for index in first..=last.min(first + 1024) {
                sheet.widths.insert(index.saturating_sub(1), width);
            }
        }
    }
    Ok(sheet)
}

/// The text of a formula. A cell of a shared group holds only the group's number, so its
/// formula comes from the group's first cell.
fn formula_of(
    formula: Xml<'_, '_>,
    row: u32,
    column: u32,
    shared: &mut HashMap<String, (String, u32, u32)>,
) -> Option<String> {
    let text = formula.text().map(str::to_owned);
    if formula.attribute("t") != Some("shared") {
        return text;
    }
    let group = formula.attribute("si")?;
    if let Some(text) = text {
        shared.insert(group.to_owned(), (text.clone(), row, column));
        return Some(text);
    }
    let (first, first_row, first_column) = shared.get(group)?;
    Some(translate_shared(
        first,
        i64::from(row) - i64::from(*first_row),
        i64::from(column) - i64::from(*first_column),
    ))
}

fn color_of(color: Option<Xml<'_, '_>>) -> Option<String> {
    color.and_then(|color| color.attribute("rgb")).map(|rgb| {
        rgb.get(rgb.len().saturating_sub(6)..)
            .unwrap_or(rgb)
            .to_owned()
    })
}

fn read_looks(text: &str) -> Result<Vec<Look>, Unreadable> {
    let parsed = Document::parse(text).map_err(broken)?;
    let root = parsed.root_element();
    let fonts: Vec<Xml<'_, '_>> = local(root, "fonts")
        .map(|fonts| locals(fonts, "font").collect())
        .unwrap_or_default();
    let fills: Vec<Xml<'_, '_>> = local(root, "fills")
        .map(|fills| locals(fills, "fill").collect())
        .unwrap_or_default();
    let number_formats: BTreeMap<&str, &str> = local(root, "numFmts")
        .map(|formats| {
            locals(formats, "numFmt")
                .filter_map(|format| {
                    Some((
                        format.attribute("numFmtId")?,
                        format.attribute("formatCode")?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let Some(formats) = local(root, "cellXfs") else {
        return Ok(Vec::new());
    };
    let index_of = |element: Xml<'_, '_>, name: &str| {
        element
            .attribute(name)
            .and_then(|value| value.parse::<usize>().ok())
    };
    Ok(locals(formats, "xf")
        .map(|format| {
            let mut look = Look::default();
            if let Some(font) = index_of(format, "fontId").and_then(|index| fonts.get(index)) {
                look.bold = local(*font, "b").is_some();
                look.italic = local(*font, "i").is_some();
                look.underline = local(*font, "u").is_some();
                look.color = color_of(local(*font, "color"));
                look.size = local(*font, "sz")
                    .and_then(|size| size.attribute("val"))
                    .and_then(|size| size.parse().ok());
                look.font = local(*font, "name")
                    .and_then(|name| name.attribute("val"))
                    .map(str::to_owned);
            }
            if let Some(fill) = index_of(format, "fillId").and_then(|index| fills.get(index)) {
                look.fill = local(*fill, "patternFill")
                    .filter(|pattern| pattern.attribute("patternType") == Some("solid"))
                    .and_then(|pattern| color_of(local(pattern, "fgColor")));
            }
            look.number_format = format
                .attribute("numFmtId")
                .and_then(|id| number_formats.get(id))
                .map(|code| (*code).to_owned());
            if let Some(alignment) = local(format, "alignment") {
                look.align = alignment.attribute("horizontal").map(str::to_owned);
                look.wrap = alignment.attribute("wrapText") == Some("1");
            }
            look
        })
        .collect())
}
