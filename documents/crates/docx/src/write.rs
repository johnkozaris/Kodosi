use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use document::{Assets, Hash, Unwritable};
use package::{Package, Parts, RELATIONSHIPS, Relation, escape, rewrite};
use roxmltree::Node as Xml;
use serde_json::Value;
use text::{Mark, Node};

use crate::{
    model::{Model, schema},
    read::{
        MAIN_PART, ParagraphLook, RunLook, is_word, is_word_attribute, paragraph_look, run_look,
        word_attribute, word_child,
    },
};

const RELATIONS_PART: &str = "word/_rels/document.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";
const HYPERLINK_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const IMAGE_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const FIRST_CHANGE_ID: u32 = 90_000;
const EMU_PER_PIXEL: f64 = 9525.0;
/// The width of the text on an A4 page with margins of an inch, in twentieths of a point.
const TEXT_WIDTH: u64 = 9026;
/// The look of a new table: as wide as its columns, with thin lines around each cell.
const NEW_TABLE: &str = concat!(
    r#"<w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>"#,
    r#"<w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
    r#"<w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
    r#"<w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
    r#"<w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
    r#"<w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
    r#"<w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
    r#"</w:tblBorders></w:tblPr>"#,
);

/// The children of `w:rPr` in their order in the schema, which readers hold a file to.
const RUN_ORDER: &[&str] = &[
    "rStyle",
    "rFonts",
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "dstrike",
    "outline",
    "shadow",
    "emboss",
    "imprint",
    "noProof",
    "snapToGrid",
    "vanish",
    "webHidden",
    "color",
    "spacing",
    "w",
    "kern",
    "position",
    "sz",
    "szCs",
    "highlight",
    "u",
    "effect",
    "bdr",
    "shd",
    "fitText",
    "vertAlign",
    "rtl",
    "cs",
    "em",
    "lang",
    "eastAsianLayout",
    "specVanish",
    "oMath",
];
const PARAGRAPH_ORDER: &[&str] = &[
    "pStyle",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "framePr",
    "widowControl",
    "numPr",
    "suppressLineNumbers",
    "pBdr",
    "shd",
    "tabs",
    "suppressAutoHyphens",
    "kinsoku",
    "wordWrap",
    "overflowPunct",
    "topLinePunct",
    "autoSpaceDE",
    "autoSpaceDN",
    "bidi",
    "adjustRightInd",
    "snapToGrid",
    "spacing",
    "ind",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
    "jc",
    "textDirection",
    "textAlignment",
    "textboxTightWrap",
    "outlineLvl",
    "divId",
    "cnfStyle",
    "rPr",
    "sectPr",
    "pPrChange",
];
const CELL_ORDER: &[&str] = &[
    "cnfStyle",
    "tcW",
    "gridSpan",
    "hMerge",
    "vMerge",
    "tcBorders",
    "shd",
    "noWrap",
    "tcMar",
    "textDirection",
    "tcFitText",
    "vAlign",
    "hideMark",
    "headers",
    "cellIns",
    "cellDel",
    "cellMerge",
    "tcPrChange",
];

fn failed(error: impl std::fmt::Display) -> Unwritable {
    Unwritable(error.to_string())
}

fn rank(order: &[&str], name: &str) -> usize {
    order
        .iter()
        .position(|known| *known == name)
        .unwrap_or(order.len())
}

struct Out<'m> {
    model: &'m Model,
    assets: &'m Assets,
    namespaces: String,
    /// The relationships of the document part, and those that this write adds, by id.
    relations: HashMap<String, Relation>,
    links: Vec<(String, String)>,
    images: Vec<(String, Hash, String)>,
    /// The width of the text on a page, in twentieths of a point, which a new table fills.
    text_width: u64,
    next_relation: u32,
    next_change: u32,
    next_picture: u32,
}

/// Whether a cell of a table goes on down into the cells below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Merge {
    Apart,
    Starts,
    Continues,
}

fn merge_of(properties: Xml<'_, '_>) -> Merge {
    match word_child(properties, "vMerge") {
        None => Merge::Apart,
        Some(merge) if word_attribute(merge, "val") == Some("restart") => Merge::Starts,
        Some(_) => Merge::Continues,
    }
}

/// Where a cell lies in the grid of its table.
struct Place {
    start: u64,
    span: u64,
    merged: bool,
}

fn places_of(row: &Node) -> Vec<Place> {
    let mut start = 0;
    row.content()
        .children()
        .iter()
        .map(|cell| {
            let span = cell
                .attrs()
                .get("span")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1);
            let merged = cell.attrs().get("merged").and_then(Value::as_bool) == Some(true);
            let place = Place {
                start,
                span,
                merged,
            };
            start += span;
            place
        })
        .collect()
}

fn text_width_of(section: Xml<'_, '_>) -> u64 {
    let twips = |name: &str, attribute: &str| {
        word_child(section, name)
            .and_then(|node| word_attribute(node, attribute))
            .and_then(|value| value.parse::<u64>().ok())
    };
    let page = twips("pgSz", "w").unwrap_or(TEXT_WIDTH + 2880);
    let margins = twips("pgMar", "left").unwrap_or(1440) + twips("pgMar", "right").unwrap_or(1440);
    page.checked_sub(margins)
        .filter(|width| *width > 0)
        .unwrap_or(TEXT_WIDTH)
}

pub(crate) fn write(model: &Model, file: &[u8], assets: &Assets) -> Result<Vec<u8>, Unwritable> {
    let mut package = Package::open(file).map_err(failed)?;
    let xml = package
        .text(MAIN_PART)
        .ok_or_else(|| Unwritable("the file has no document part".to_owned()))?;
    let parsed = roxmltree::Document::parse(&xml).map_err(failed)?;
    let root = parsed.root_element();
    let body = root
        .children()
        .find(|child| is_word(*child, "body"))
        .ok_or_else(|| Unwritable("the document has no body".to_owned()))?;
    let section = body.children().rfind(|child| is_word(*child, "sectPr"));
    let mut out = Out {
        model,
        assets,
        namespaces: namespaces_of(root),
        relations: package.relations(MAIN_PART),
        links: Vec::new(),
        images: Vec::new(),
        text_width: section.map_or(TEXT_WIDTH, text_width_of),
        next_relation: 1,
        next_change: FIRST_CHANGE_ID,
        next_picture: 1,
    };
    let mut blocks = String::with_capacity(xml.len());
    for block in model.doc.content().children() {
        blocks.push_str(&out.block(block).map_err(failed)?);
    }
    let start = body
        .first_child()
        .map_or_else(|| body.range().end, |child| child.range().start);
    let end = section.map_or_else(
        || {
            let tail = &xml[body.range()];
            let closing = tail.rfind("</").unwrap_or(tail.len());
            body.range().start + closing
        },
        |section| section.range().start,
    );
    let new_xml = format!("{}{blocks}{}", &xml[..start], &xml[end..]);
    if new_xml == xml {
        return Ok(file.to_vec());
    }
    let mut parts = Parts::from([(MAIN_PART.to_owned(), Some(new_xml.into_bytes()))]);
    out.finish_parts(&mut package, &mut parts).map_err(failed)?;
    rewrite(file, &parts).map_err(failed)
}

fn namespaces_of(root: Xml<'_, '_>) -> String {
    let mut text = String::new();
    for namespace in root.namespaces() {
        if let Some(name) = namespace.name() {
            let _ = write!(text, r#" xmlns:{name}="{}""#, escape(namespace.uri()));
        }
    }
    text
}

fn text_attr<'n>(node: &'n Node, name: &str) -> Option<&'n str> {
    node.attrs().get(name).and_then(Value::as_str)
}

fn mark_named<'m>(marks: &'m [Mark], name: &str) -> Option<&'m Mark> {
    marks
        .iter()
        .find(|mark| schema().mark_name(mark.kind()) == name)
}

fn mark_text<'m>(mark: &'m Mark, name: &str) -> Option<&'m str> {
    mark.attrs().get(name).and_then(Value::as_str)
}

fn look_of(marks: &[Mark]) -> RunLook {
    let style = mark_named(marks, "textStyle");
    let value = |name: &str| {
        style
            .and_then(|mark| mark_text(mark, name))
            .map(str::to_owned)
    };
    RunLook {
        bold: mark_named(marks, "bold").is_some(),
        italic: mark_named(marks, "italic").is_some(),
        underline: mark_named(marks, "underline").is_some(),
        strike: mark_named(marks, "strike").is_some(),
        superscript: mark_named(marks, "superscript").is_some(),
        subscript: mark_named(marks, "subscript").is_some(),
        color: value("color"),
        size: value("size"),
        font: value("font"),
        highlight: value("highlight"),
    }
}

/// The marks that wrap runs in an element of their own.
#[derive(PartialEq, Eq, Clone)]
struct Wrappers {
    link: Option<Mark>,
    insertion: Option<Mark>,
    deletion: Option<Mark>,
}

impl Wrappers {
    fn of(node: &Node) -> Self {
        Self {
            link: mark_named(node.marks(), "link").cloned(),
            insertion: mark_named(node.marks(), "insertion").cloned(),
            deletion: mark_named(node.marks(), "deletion").cloned(),
        }
    }
}

impl Out<'_> {
    fn raw(&self, name: &str) -> Result<&str, String> {
        self.model
            .raws
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("the piece {name} is missing"))
    }

    fn block(&mut self, node: &Node) -> Result<String, String> {
        if let Some(id) = text_attr(node, "id")
            && let Some(source) = self.model.sources.get(id)
            && source.node == *node
        {
            return Ok(source.xml.clone());
        }
        match schema().node_name(node.kind()) {
            "paragraph" => self.paragraph(node),
            "table" => self.table(node),
            "opaque" => self
                .raw(text_attr(node, "raw").unwrap_or_default())
                .map(str::to_owned),
            other => Err(format!("{other} cannot stand as a block")),
        }
    }

    fn parse_piece<T>(&self, xml: &str, read: impl FnOnce(Xml<'_, '_>) -> T) -> Result<T, String> {
        self.parse_pieces(xml, |pieces| pieces.first_element_child().map(read))?
            .ok_or_else(|| "an empty piece".to_owned())
    }

    /// Reads pieces of XML side by side, under one element that holds them.
    fn parse_pieces<T>(&self, xml: &str, read: impl FnOnce(Xml<'_, '_>) -> T) -> Result<T, String> {
        let wrapped = format!("<piece{}>{xml}</piece>", self.namespaces);
        let parsed = roxmltree::Document::parse(&wrapped).map_err(|error| error.to_string())?;
        Ok(read(parsed.root_element()))
    }

    fn paragraph_props(&mut self, node: &Node) -> Result<String, String> {
        let raw = match text_attr(node, "props") {
            Some(name) => self.raw(name)?.to_owned(),
            None => String::new(),
        };
        let wanted = |name: &str| text_attr(node, name).map(str::to_owned);
        let change = |name: &str| node.attrs().get(name).cloned().unwrap_or(Value::Null);
        let wanted = ParagraphLook {
            style: wanted("style"),
            align: wanted("align"),
            list: wanted("numId"),
            level: wanted("ilvl"),
            line: wanted("line"),
            indent: wanted("indent"),
            inserted: change("inserted"),
            deleted: change("deleted"),
        };
        let changes = self.mark_changes(&wanted);
        if raw.is_empty() {
            let mut children = Vec::new();
            add_paragraph_look(&wanted, &mut children);
            add_mark_props(&changes, &mut children);
            return Ok(wrapped("pPr", children));
        }
        self.parse_piece(&raw, |properties| {
            let current = paragraph_look(properties);
            if current == wanted {
                return raw.clone();
            }
            let mut marked =
                (&current.inserted, &current.deleted) == (&wanted.inserted, &wanted.deleted);
            let mut missing = wanted;
            let mut children = Vec::new();
            for child in properties.children().filter(Xml::is_element) {
                if ["pStyle", "jc", "numPr"]
                    .iter()
                    .any(|name| is_word(child, name))
                {
                    continue;
                }
                let xml = &raw[shifted(properties, child)];
                let kept = if is_word(child, "spacing") {
                    let line = missing.line.take();
                    if current.line == line {
                        Some(xml.to_owned())
                    } else {
                        let line = line.as_deref().map_or_else(Vec::new, |line| {
                            vec![("line", line), ("lineRule", "auto")]
                        });
                        with_attributes(child, xml, &["line", "lineRule"], &line)
                    }
                } else if is_word(child, "ind") {
                    let indent = missing.indent.take();
                    if current.indent == indent {
                        Some(xml.to_owned())
                    } else {
                        let side = if word_attribute(child, "start").is_some() {
                            "start"
                        } else {
                            "left"
                        };
                        let indent = indent
                            .as_deref()
                            .map_or_else(Vec::new, |indent| vec![(side, indent)]);
                        let gone = ["start", "left", "startChars", "leftChars"];
                        with_attributes(child, xml, &gone, &indent)
                    }
                } else if is_word(child, "rPr") && !marked {
                    marked = true;
                    mark_props(child, xml, &changes)
                } else {
                    Some(xml.to_owned())
                };
                if let Some(xml) = kept {
                    children.push((rank(PARAGRAPH_ORDER, child.tag_name().name()), xml));
                }
            }
            add_paragraph_look(&missing, &mut children);
            if !marked {
                add_mark_props(&changes, &mut children);
            }
            wrapped("pPr", children)
        })
    }

    /// The tracked changes of the mark that ends a paragraph, each with an id of its own.
    fn mark_changes(&mut self, look: &ParagraphLook) -> String {
        let mut xml = String::new();
        for (tag, change) in [("ins", &look.inserted), ("del", &look.deleted)] {
            if change.is_object() {
                let text =
                    |name: &str| change.get(name).and_then(Value::as_str).unwrap_or_default();
                let attributes = self.change_attributes(text("author"), text("date"));
                let _ = write!(xml, "<w:{tag}{attributes}/>");
            }
        }
        xml
    }

    fn paragraph(&mut self, node: &Node) -> Result<String, String> {
        let mut xml = format!("<w:p>{}", self.paragraph_props(node)?);
        let children = node.content().children();
        let mut index = 0;
        while index < children.len() {
            let wrappers = Wrappers::of(&children[index]);
            let end = children[index..]
                .iter()
                .position(|child| Wrappers::of(child) != wrappers)
                .map_or(children.len(), |offset| index + offset);
            xml.push_str(&self.wrapped_runs(&children[index..end], &wrappers)?);
            index = end;
        }
        xml.push_str("</w:p>");
        Ok(xml)
    }

    fn wrapped_runs(&mut self, nodes: &[Node], wrappers: &Wrappers) -> Result<String, String> {
        let deleted = wrappers.deletion.is_some();
        let mut runs = String::new();
        for node in nodes {
            runs.push_str(&self.inline(node, deleted)?);
        }
        if let Some(link) = &wrappers.link {
            let open = self.link_open(link);
            runs = format!("{open}{runs}</w:hyperlink>");
        }
        if let Some(deletion) = &wrappers.deletion {
            let open = self.change_open("del", deletion);
            runs = format!("{open}{runs}</w:del>");
        }
        if let Some(insertion) = &wrappers.insertion {
            let open = self.change_open("ins", insertion);
            runs = format!("{open}{runs}</w:ins>");
        }
        Ok(runs)
    }

    fn change_open(&mut self, tag: &str, mark: &Mark) -> String {
        let attributes = self.change_attributes(
            mark_text(mark, "author").unwrap_or_default(),
            mark_text(mark, "date").unwrap_or_default(),
        );
        format!("<w:{tag}{attributes}>")
    }

    /// A new id, the author and the date of a tracked change. A change with no date has none.
    fn change_attributes(&mut self, author: &str, date: &str) -> String {
        let id = self.next_change;
        self.next_change += 1;
        let mut attributes = format!(r#" w:id="{id}" w:author="{}""#, escape(author));
        if !date.is_empty() {
            let _ = write!(attributes, r#" w:date="{}""#, escape(date));
        }
        attributes
    }

    fn link_open(&mut self, link: &Mark) -> String {
        let id = if let Some(relation) = mark_text(link, "relation") {
            relation.to_owned()
        } else {
            let href = mark_text(link, "href").unwrap_or_default().to_owned();
            let (id, new) = self.relation_to(HYPERLINK_KIND, &href, true);
            if new {
                self.links.push((id.clone(), href));
            }
            id
        };
        format!(
            r#"<w:hyperlink xmlns:r="{RELATIONSHIPS}" r:id="{}">"#,
            escape(&id)
        )
    }

    /// The relationship to a target: the one that the file has, or a new one that the write
    /// adds. A later write finds the one that an earlier write added.
    fn relation_to(&mut self, kind: &str, target: &str, external: bool) -> (String, bool) {
        let known = self
            .relations
            .iter()
            .filter(|(_, relation)| {
                relation.kind == kind && relation.target == target && relation.external == external
            })
            .map(|(id, _)| id)
            .min();
        if let Some(id) = known {
            return (id.clone(), false);
        }
        let id = loop {
            let id = format!("rIdDoc{}", self.next_relation);
            self.next_relation += 1;
            if !self.relations.contains_key(&id) {
                break id;
            }
        };
        self.relations.insert(
            id.clone(),
            Relation {
                kind: kind.to_owned(),
                target: target.to_owned(),
                external,
            },
        );
        (id, true)
    }

    fn inline(&mut self, node: &Node, deleted: bool) -> Result<String, String> {
        if let Some(text) = node.text_of() {
            return self.text_run(&text, node.marks(), deleted);
        }
        match schema().node_name(node.kind()) {
            "hardBreak" => Ok(format!(
                "<w:r>{}<w:br/></w:r>",
                self.run_props(node.marks())?
            )),
            "pageBreak" => Ok(format!(
                r#"<w:r>{}<w:br w:type="page"/></w:r>"#,
                self.run_props(node.marks())?
            )),
            "tab" => Ok(format!(
                "<w:r>{}<w:tab/></w:r>",
                self.run_props(node.marks())?
            )),
            "inlineOpaque" => self
                .raw(text_attr(node, "raw").unwrap_or_default())
                .map(str::to_owned),
            "image" => match text_attr(node, "raw") {
                Some(raw) => self.raw(raw).map(str::to_owned),
                None => self.new_image(node),
            },
            other => Err(format!("{other} cannot stand in a paragraph")),
        }
    }

    fn text_run(&self, text: &str, marks: &[Mark], deleted: bool) -> Result<String, String> {
        let tag = if deleted { "w:delText" } else { "w:t" };
        let mut inner = String::with_capacity(text.len() + 32);
        let mut piece = String::new();
        let flush = |piece: &mut String, inner: &mut String| {
            if !piece.is_empty() {
                let _ = write!(
                    inner,
                    r#"<{tag} xml:space="preserve">{}</{tag}>"#,
                    escape(piece)
                );
                piece.clear();
            }
        };
        for letter in text.chars() {
            match letter {
                '\t' => {
                    flush(&mut piece, &mut inner);
                    inner.push_str("<w:tab/>");
                }
                '\n' => {
                    flush(&mut piece, &mut inner);
                    inner.push_str("<w:br/>");
                }
                letter => piece.push(letter),
            }
        }
        flush(&mut piece, &mut inner);
        Ok(format!("<w:r>{}{inner}</w:r>", self.run_props(marks)?))
    }

    fn run_props(&self, marks: &[Mark]) -> Result<String, String> {
        let wanted = look_of(marks);
        let raw = match mark_named(marks, "run").and_then(|mark| mark_text(mark, "props")) {
            Some(name) => self.raw(name)?.to_owned(),
            None => String::new(),
        };
        if raw.is_empty() {
            let mut children = Vec::new();
            add_run_look(&wanted, &mut children);
            return Ok(wrapped("rPr", children));
        }
        self.parse_piece(&raw, |properties| {
            if run_look(properties) == wanted {
                return raw.clone();
            }
            let managed = [
                "b",
                "i",
                "u",
                "strike",
                "vertAlign",
                "color",
                "sz",
                "highlight",
            ];
            let keep_fonts = run_look(properties).font == wanted.font;
            let mut children: Vec<(usize, String)> = properties
                .children()
                .filter(Xml::is_element)
                .filter(|child| {
                    let fonts = is_word(*child, "rFonts");
                    (!fonts || keep_fonts) && !managed.iter().any(|name| is_word(*child, name))
                })
                .map(|child| {
                    let name = child.tag_name().name();
                    (
                        rank(RUN_ORDER, name),
                        raw[shifted(properties, child)].to_owned(),
                    )
                })
                .collect();
            let mut look = wanted.clone();
            if keep_fonts {
                look.font = None;
            }
            add_run_look(&look, &mut children);
            wrapped("rPr", children)
        })
    }

    fn table(&mut self, node: &Node) -> Result<String, String> {
        let rows = node.content().children();
        let places: Vec<Vec<Place>> = rows.iter().map(places_of).collect();
        let columns = places
            .iter()
            .filter_map(|row| row.last().map(|cell| cell.start + cell.span))
            .max()
            .unwrap_or(1);
        let mut xml = format!("<w:tbl>{}", self.table_props(node, columns)?);
        for (index, row) in rows.iter().enumerate() {
            xml.push_str("<w:tr>");
            if let Some(name) = text_attr(row, "props") {
                xml.push_str(self.raw(name)?);
            }
            let below = places.get(index + 1);
            for (cell, place) in row.content().children().iter().zip(&places[index]) {
                let merge = if place.merged {
                    Merge::Continues
                } else if below.is_some_and(|below| {
                    below
                        .iter()
                        .any(|other| other.start == place.start && other.merged)
                }) {
                    Merge::Starts
                } else {
                    Merge::Apart
                };
                xml.push_str("<w:tc>");
                xml.push_str(&self.cell_props(cell, place.span, merge)?);
                let blocks = cell.content().children();
                for block in blocks {
                    let inner = self.block(block)?;
                    xml.push_str(&inner);
                }
                if blocks
                    .last()
                    .is_some_and(|block| schema().node_name(block.kind()) == "table")
                {
                    xml.push_str("<w:p/>");
                }
                xml.push_str("</w:tc>");
            }
            xml.push_str("</w:tr>");
        }
        xml.push_str("</w:tbl>");
        Ok(xml)
    }

    /// The properties and grid of a table. The grid keeps its widths while it has a column for
    /// each column of the cells, and else shares their sum out again.
    fn table_props(&self, node: &Node, columns: u64) -> Result<String, String> {
        let raw = match text_attr(node, "props") {
            Some(name) => self.raw(name)?.to_owned(),
            None => return Ok(format!("{NEW_TABLE}{}", grid_of(columns, self.text_width))),
        };
        self.parse_pieces(&raw, |pieces| {
            let children: Vec<Xml<'_, '_>> = pieces.children().filter(Xml::is_element).collect();
            let start = children.first().map_or(0, |child| child.range().start);
            let mut xml = String::new();
            if !children.iter().any(|child| is_word(*child, "tblPr")) {
                xml.push_str("<w:tblPr/>");
            }
            let grid = children.iter().find(|child| is_word(**child, "tblGrid"));
            let widths: Vec<u64> = grid.map_or_else(Vec::new, |grid| {
                grid.children()
                    .filter(|column| is_word(*column, "gridCol"))
                    .map(|column| {
                        word_attribute(column, "w")
                            .and_then(|width| width.parse().ok())
                            .unwrap_or(0)
                    })
                    .collect()
            });
            let total: u64 = widths.iter().sum();
            let new_grid = grid_of(columns, if total > 0 { total } else { self.text_width });
            match grid {
                Some(_) if widths.len() as u64 == columns => xml.push_str(&raw),
                Some(grid) => {
                    let range = grid.range();
                    xml.push_str(&raw[..range.start - start]);
                    xml.push_str(&new_grid);
                    xml.push_str(&raw[range.end - start..]);
                }
                None => {
                    xml.push_str(&raw);
                    xml.push_str(&new_grid);
                }
            }
            xml
        })
    }

    /// The properties of a cell, with the span and the vertical merge that its place gives.
    fn cell_props(&self, cell: &Node, span: u64, merge: Merge) -> Result<String, String> {
        let mut children = Vec::new();
        if let Some(name) = text_attr(cell, "props") {
            let raw = self.raw(name)?;
            let kept = self.parse_piece(raw, |properties| {
                let now = word_child(properties, "gridSpan")
                    .and_then(|node| word_attribute(node, "val"))
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(1);
                if now == span && merge_of(properties) == merge {
                    return None;
                }
                Some(
                    properties
                        .children()
                        .filter(Xml::is_element)
                        .filter(|child| !is_word(*child, "gridSpan") && !is_word(*child, "vMerge"))
                        .map(|child| {
                            (
                                rank(CELL_ORDER, child.tag_name().name()),
                                raw[shifted(properties, child)].to_owned(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })?;
            match kept {
                Some(kept) => children = kept,
                None => return Ok(raw.to_owned()),
            }
        }
        if span > 1 {
            children.push((
                rank(CELL_ORDER, "gridSpan"),
                format!(r#"<w:gridSpan w:val="{span}"/>"#),
            ));
        }
        let vertical = match merge {
            Merge::Apart => None,
            Merge::Starts => Some(r#"<w:vMerge w:val="restart"/>"#),
            Merge::Continues => Some("<w:vMerge/>"),
        };
        if let Some(vertical) = vertical {
            children.push((rank(CELL_ORDER, "vMerge"), vertical.to_owned()));
        }
        Ok(wrapped("tcPr", children))
    }

    fn new_image(&mut self, node: &Node) -> Result<String, String> {
        let asset = text_attr(node, "asset").ok_or("an image has no asset")?;
        let hash = hash_from_hex(asset).ok_or("the asset name is not a hash")?;
        let bytes = self
            .assets
            .get(&hash)
            .ok_or_else(|| format!("the asset {asset} is missing"))?;
        let extension = picture_extension(bytes).ok_or("this kind of picture cannot go in")?;
        let number = |name: &str| {
            node.attrs()
                .get(name)
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        };
        let media = media_of(&hash, extension);
        let (id, new) = self.relation_to(IMAGE_KIND, &format!("word/{media}"), false);
        if new {
            self.images.push((id.clone(), hash, extension.to_owned()));
        }
        let picture = self.next_picture;
        self.next_picture += 1;
        Ok(picture_run(
            &id,
            picture,
            number("width") * EMU_PER_PIXEL,
            number("height") * EMU_PER_PIXEL,
        ))
    }

    fn finish_parts(self, package: &mut Package<'_>, parts: &mut Parts) -> Result<(), String> {
        if self.links.is_empty() && self.images.is_empty() {
            return Ok(());
        }
        let relations = package
            .text(RELATIONS_PART)
            .ok_or("the document has no relationships")?;
        let mut added = String::new();
        for (id, href) in &self.links {
            let _ = write!(
                added,
                r#"<Relationship Id="{id}" Type="{HYPERLINK_KIND}" Target="{}" TargetMode="External"/>"#,
                escape(href)
            );
        }
        let mut extensions = HashSet::new();
        for (id, hash, extension) in &self.images {
            let target = media_of(hash, extension);
            let _ = write!(
                added,
                r#"<Relationship Id="{id}" Type="{IMAGE_KIND}" Target="{target}"/>"#
            );
            let bytes = self.assets.get(hash).ok_or("an asset is missing")?;
            parts.insert(format!("word/{target}"), Some(bytes.to_vec()));
            extensions.insert(extension.clone());
        }
        let closing = relations
            .rfind("</")
            .ok_or("the relationships are broken")?;
        let updated = format!("{}{added}{}", &relations[..closing], &relations[closing..]);
        parts.insert(RELATIONS_PART.to_owned(), Some(updated.into_bytes()));
        if !extensions.is_empty() {
            let types = package
                .text(CONTENT_TYPES)
                .ok_or("the content types are missing")?;
            let mut known = types.clone();
            for extension in &extensions {
                if !types.contains(&format!(r#"Extension="{extension}""#)) {
                    let closing = known.rfind("</").ok_or("the content types are broken")?;
                    known.insert_str(
                        closing,
                        &format!(
                            r#"<Default Extension="{extension}" ContentType="image/{extension}"/>"#
                        ),
                    );
                }
            }
            parts.insert(CONTENT_TYPES.to_owned(), Some(known.into_bytes()));
        }
        Ok(())
    }
}

/// The range of a child in the text of its piece, from a range in the wrapped document.
fn shifted(properties: Xml<'_, '_>, child: Xml<'_, '_>) -> std::ops::Range<usize> {
    let start = properties.range().start;
    child.range().start - start..child.range().end - start
}

/// An element of a piece with some of its attributes gone and others added, or nothing when it
/// has none left.
fn with_attributes(
    element: Xml<'_, '_>,
    xml: &str,
    gone: &[&str],
    added: &[(&str, &str)],
) -> Option<String> {
    let start = element.range().start;
    let mut text = xml.to_owned();
    let mut left = added.len();
    let attributes: Vec<_> = element.attributes().collect();
    for attribute in attributes.iter().rev() {
        if !is_word_attribute(attribute) || !gone.contains(&attribute.name()) {
            left += 1;
            continue;
        }
        let range = attribute.range();
        let end = range.end - start;
        let from = text[..range.start - start].trim_end().len();
        text.replace_range(from..end, "");
    }
    if left == 0 {
        return None;
    }
    let name_end = text
        .find(|letter: char| letter.is_ascii_whitespace() || letter == '/' || letter == '>')
        .unwrap_or(text.len());
    let new = added.iter().fold(String::new(), |mut new, (name, value)| {
        let _ = write!(new, r#" w:{name}="{}""#, escape(value));
        new
    });
    text.insert_str(name_end, &new);
    Some(text)
}

/// The properties that a save writes for the look of a paragraph alone.
pub(crate) fn look_properties(look: &ParagraphLook) -> String {
    let mut children = Vec::new();
    add_paragraph_look(look, &mut children);
    wrapped("pPr", children)
}

fn add_paragraph_look(look: &ParagraphLook, children: &mut Vec<(usize, String)>) {
    let mut add = |name: &str, xml: String| children.push((rank(PARAGRAPH_ORDER, name), xml));
    if let Some(style) = &look.style {
        add(
            "pStyle",
            format!(r#"<w:pStyle w:val="{}"/>"#, escape(style)),
        );
    }
    if let Some(list) = &look.list {
        add(
            "numPr",
            format!(
                r#"<w:numPr><w:ilvl w:val="{}"/><w:numId w:val="{}"/></w:numPr>"#,
                escape(look.level.as_deref().unwrap_or("0")),
                escape(list)
            ),
        );
    }
    if let Some(line) = &look.line {
        add(
            "spacing",
            format!(
                r#"<w:spacing w:line="{}" w:lineRule="auto"/>"#,
                escape(line)
            ),
        );
    }
    if let Some(indent) = &look.indent {
        add("ind", format!(r#"<w:ind w:left="{}"/>"#, escape(indent)));
    }
    if let Some(align) = &look.align {
        add("jc", format!(r#"<w:jc w:val="{}"/>"#, escape(align)));
    }
}

fn add_run_look(look: &RunLook, children: &mut Vec<(usize, String)>) {
    let mut add = |name: &str, xml: String| children.push((rank(RUN_ORDER, name), xml));
    if let Some(font) = &look.font {
        let font = escape(font);
        add(
            "rFonts",
            format!(r#"<w:rFonts w:ascii="{font}" w:hAnsi="{font}"/>"#),
        );
    }
    if look.bold {
        add("b", "<w:b/>".to_owned());
    }
    if look.italic {
        add("i", "<w:i/>".to_owned());
    }
    if look.strike {
        add("strike", "<w:strike/>".to_owned());
    }
    if let Some(color) = &look.color {
        add("color", format!(r#"<w:color w:val="{}"/>"#, escape(color)));
    }
    if let Some(size) = &look.size {
        add("sz", format!(r#"<w:sz w:val="{}"/>"#, escape(size)));
    }
    if let Some(highlight) = &look.highlight {
        add(
            "highlight",
            format!(r#"<w:highlight w:val="{}"/>"#, escape(highlight)),
        );
    }
    if look.underline {
        add("u", r#"<w:u w:val="single"/>"#.to_owned());
    }
    if look.superscript {
        add(
            "vertAlign",
            r#"<w:vertAlign w:val="superscript"/>"#.to_owned(),
        );
    } else if look.subscript {
        add(
            "vertAlign",
            r#"<w:vertAlign w:val="subscript"/>"#.to_owned(),
        );
    }
}

/// The properties of the mark that ends a paragraph, with these tracked changes first in place of
/// its own, or nothing when none are left.
fn mark_props(properties: Xml<'_, '_>, xml: &str, changes: &str) -> Option<String> {
    let kept: String = properties
        .children()
        .filter(|child| child.is_element() && !is_word(*child, "ins") && !is_word(*child, "del"))
        .map(|child| &xml[shifted(properties, child)])
        .collect();
    let inner = format!("{changes}{kept}");
    (!inner.is_empty()).then(|| format!("<w:rPr>{inner}</w:rPr>"))
}

fn add_mark_props(changes: &str, children: &mut Vec<(usize, String)>) {
    if !changes.is_empty() {
        children.push((
            rank(PARAGRAPH_ORDER, "rPr"),
            format!("<w:rPr>{changes}</w:rPr>"),
        ));
    }
}

/// Properties in the order of the schema, in their element, or nothing when there are none.
fn wrapped(name: &str, mut children: Vec<(usize, String)>) -> String {
    if children.is_empty() {
        return String::new();
    }
    children.sort_by_key(|(rank, _)| *rank);
    let inner: String = children.into_iter().map(|(_, xml)| xml).collect();
    format!("<w:{name}>{inner}</w:{name}>")
}

/// A grid of columns of one width that fill a table's width, in twentieths of a point.
fn grid_of(columns: u64, width: u64) -> String {
    let columns = columns.max(1);
    let mut xml = "<w:tblGrid>".to_owned();
    for _ in 0..columns {
        let _ = write!(xml, r#"<w:gridCol w:w="{}"/>"#, width / columns);
    }
    xml.push_str("</w:tblGrid>");
    xml
}

/// The part of a new picture, beside the document part.
fn media_of(hash: &Hash, extension: &str) -> String {
    let name: String = hash.iter().take(8).fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    });
    format!("media/{name}.{extension}")
}

fn hash_from_hex(text: &str) -> Option<Hash> {
    let mut hash = [0; 32];
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    for (index, pair) in text.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(pair).ok()?;
        hash[index] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(hash)
}

fn picture_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else {
        None
    }
}

fn picture_run(relation: &str, picture: u32, width: f64, height: f64) -> String {
    format!(
        concat!(
            r#"<w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0">"#,
            r#"<wp:extent cx="{width:.0}" cy="{height:.0}"/><wp:docPr id="{id}" name="Picture {id}"/>"#,
            r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="{id}" name="Picture {id}"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip xmlns:r="{relationship}" r:embed="{relation}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{width:.0}" cy="{height:.0}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            r#"</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
        ),
        width = width,
        height = height,
        id = 9000 + picture,
        relationship = RELATIONSHIPS,
        relation = relation,
    )
}
