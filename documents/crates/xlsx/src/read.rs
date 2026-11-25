use std::collections::{BTreeMap, BTreeSet, HashMap};

use document::Unreadable;
use package::{Package, Relation, escape, relation_id};
use roxmltree::{Document, Node as Xml};

use crate::{
    formats,
    formula::{parse_cell, parse_range, translate_shared},
    model::{Anchor, Cell, DefinedName, Frozen, Link, Look, Model, Sheet, Value},
};

pub(crate) const WORKBOOK_PART: &str = "xl/workbook.xml";
pub(crate) const STYLES_PART: &str = "xl/styles.xml";
const STRINGS_PART: &str = "xl/sharedStrings.xml";
pub(crate) const ANCHORS: [(&str, &str); 4] = [
    ("autoFilter", "ref"),
    ("conditionalFormatting", "sqref"),
    ("dataValidation", "sqref"),
    ("hyperlink", "ref"),
];
const ROW_FACTS: [&str; 5] = ["r", "spans", "ht", "customHeight", "hidden"];
const COLUMN_FACTS: [&str; 5] = ["min", "max", "width", "customWidth", "hidden"];
const MOST_COLUMNS_PER_ENTRY: u32 = 1024;

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
        let part = relation_id(entry, "id")
            .and_then(|id| relations.get(id))
            .map(|relation| relation.target.clone());
        let Some(part) = part else {
            continue;
        };
        let Some(xml) = package.text(&part) else {
            continue;
        };
        let mut sheet = read_sheet(&xml, &strings, &package.relations(&part))?;
        entry
            .attribute("sheetId")
            .unwrap_or_default()
            .clone_into(&mut sheet.id);
        entry
            .attribute("name")
            .unwrap_or_default()
            .clone_into(&mut sheet.name);
        sheet.number = sheet.id.parse().unwrap_or_default();
        sheet.part = Some(part);
        sheets.push(sheet);
    }
    let names = local(workbook.root_element(), "definedNames").map_or_else(Vec::new, |names| {
        locals(names, "definedName")
            .map(|name| DefinedName {
                name: name.attribute("name").unwrap_or_default().to_owned(),
                sheet: name
                    .attribute("localSheetId")
                    .and_then(|place| place.parse().ok()),
                text: name.text().unwrap_or_default().to_owned(),
            })
            .collect()
    });
    let last_number = sheets.iter().map(|sheet| sheet.number).max().unwrap_or(0);
    let looks = package
        .text(STYLES_PART)
        .map(|text| read_looks(&text))
        .transpose()?
        .unwrap_or_default();
    let dates_from_1904 = local(workbook.root_element(), "workbookPr")
        .and_then(|properties| properties.attribute("date1904"))
        .is_some_and(|value| matches!(value, "1" | "true"));
    Ok(Model {
        sheets,
        looks,
        names,
        last_number,
        dates_from_1904,
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

/// Drops namespace-prefixed attributes because their prefixes may not be in scope when rewritten.
fn kept_attributes(element: Xml<'_, '_>, read_here: &[&str]) -> String {
    element
        .attributes()
        .filter(|attribute| attribute.namespace().is_none())
        .filter(|attribute| !read_here.contains(&attribute.name()))
        .map(|attribute| format!(r#"{}="{}""#, attribute.name(), escape(attribute.value())))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn is_on(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true"))
}

fn column_entries<'a, 'input: 'a>(
    root: Xml<'a, 'input>,
) -> impl Iterator<Item = (std::ops::RangeInclusive<u32>, Xml<'a, 'input>)> {
    local(root, "cols").into_iter().flat_map(|columns| {
        locals(columns, "col").filter_map(|column| {
            let number = |name: &str| column.attribute(name)?.parse::<u32>().ok();
            let first = number("min")?.checked_sub(1)?;
            let last = number("max")?.checked_sub(1)?;
            Some((first..=last.min(first + MOST_COLUMNS_PER_ENTRY), column))
        })
    })
}

pub(crate) fn widths_of(root: Xml<'_, '_>) -> BTreeMap<u32, f64> {
    column_entries(root)
        .filter_map(|(lines, column)| {
            let width = column.attribute("width")?.parse::<f64>().ok()?;
            Some(lines.map(move |index| (index, width)))
        })
        .flatten()
        .collect()
}

pub(crate) fn hidden_columns_of(root: Xml<'_, '_>) -> BTreeSet<u32> {
    column_entries(root)
        .filter(|(_, column)| is_on(column.attribute("hidden")))
        .flat_map(|(lines, _)| lines)
        .collect()
}

pub(crate) fn frozen_of(root: Xml<'_, '_>) -> Frozen {
    let Some(view) = local(root, "sheetViews").and_then(|views| local(views, "sheetView")) else {
        return Frozen::default();
    };
    let Some(pane) = local(view, "pane")
        .filter(|pane| matches!(pane.attribute("state"), Some("frozen" | "frozenSplit")))
    else {
        return Frozen::default();
    };
    let lines = |name: &str| {
        pane.attribute(name)
            .and_then(|lines| lines.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let (top, left) = view
        .attribute("topLeftCell")
        .and_then(parse_cell)
        .unwrap_or((0, 0));
    Frozen::new(top, left, lines("ySplit"), lines("xSplit"))
}

pub(crate) fn merges_of(root: Xml<'_, '_>) -> Vec<[u32; 4]> {
    local(root, "mergeCells").map_or_else(Vec::new, |merges| {
        locals(merges, "mergeCell")
            .filter_map(|merge| parse_range(merge.attribute("ref")?))
            .collect()
    })
}

fn read_sheet(
    xml: &str,
    strings: &[String],
    relations: &HashMap<String, Relation>,
) -> Result<Sheet, Unreadable> {
    let parsed = Document::parse(xml).map_err(broken)?;
    let root = parsed.root_element();
    let data = local(root, "sheetData");
    let mut sheet = Sheet {
        chart: data.is_none(),
        ..Sheet::default()
    };
    if let Some(data) = data {
        let mut shared: HashMap<String, (String, u32, u32)> = HashMap::new();
        for row in locals(data, "row") {
            if let Some(line) = row
                .attribute("r")
                .and_then(|row| row.parse::<u32>().ok())
                .and_then(|row| row.checked_sub(1))
            {
                if let Some(height) = row.attribute("ht").and_then(|height| height.parse().ok()) {
                    sheet.heights.insert(line, height);
                }
                if is_on(row.attribute("hidden")) {
                    sheet.hidden_rows.insert(line);
                }
                let look = kept_attributes(row, &ROW_FACTS);
                if !look.is_empty() {
                    sheet.row_looks.insert(line, look);
                }
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
    sheet.merges = merges_of(root);
    sheet.widths = widths_of(root);
    sheet.hidden_columns = hidden_columns_of(root);
    sheet.frozen = frozen_of(root);
    for (lines, column) in column_entries(root) {
        let look = kept_attributes(column, &COLUMN_FACTS);
        if !look.is_empty() {
            sheet
                .column_looks
                .extend(lines.map(|index| (index, look.clone())));
        }
    }
    sheet.anchors = root
        .descendants()
        .filter(Xml::is_element)
        .filter_map(|element| {
            let (tag, attribute) = ANCHORS
                .iter()
                .find(|(tag, _)| *tag == element.tag_name().name())?;
            Some(Anchor {
                tag,
                attribute,
                ranges: element.attribute(*attribute)?.to_owned(),
                link: (*tag == "hyperlink")
                    .then(|| link_of(element, relations))
                    .flatten(),
            })
        })
        .collect();
    Ok(sheet)
}

fn link_of(element: Xml<'_, '_>, relations: &HashMap<String, Relation>) -> Option<Link> {
    let relation = relation_id(element, "id").and_then(|id| relations.get(id));
    match (relation, element.attribute("location")) {
        (Some(relation), _) if relation.external => Some(Link::Href(relation.target.clone())),
        (_, Some(location)) if !location.is_empty() => Some(Link::Location(location.to_owned())),
        _ => None,
    }
}

/// Resolves OOXML shared formulas (`t="shared"`): follower cells reference the group's `si` and
/// translate the master cell's formula by their row/column offset.
pub(crate) fn formula_of(
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
    let fonts: Vec<Xml<'_, '_>> =
        local(root, "fonts").map_or_default(|fonts| locals(fonts, "font").collect());
    let fills: Vec<Xml<'_, '_>> =
        local(root, "fills").map_or_default(|fills| locals(fills, "fill").collect());
    let number_formats: BTreeMap<&str, &str> = local(root, "numFmts").map_or_default(|formats| {
        locals(formats, "numFmt")
            .filter_map(|format| {
                Some((
                    format.attribute("numFmtId")?,
                    format.attribute("formatCode")?,
                ))
            })
            .collect()
    });
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
                look.strike = local(*font, "strike").is_some();
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
            look.number_format = format.attribute("numFmtId").and_then(|id| {
                number_formats
                    .get(id)
                    .copied()
                    .or_else(|| formats::built_in_code(id.parse().ok()?))
                    .map(str::to_owned)
            });
            if let Some(alignment) = local(format, "alignment") {
                look.align = alignment.attribute("horizontal").map(str::to_owned);
                look.vertical = alignment.attribute("vertical").map(str::to_owned);
                look.wrap = alignment.attribute("wrapText") == Some("1");
            }
            look
        })
        .collect())
}
