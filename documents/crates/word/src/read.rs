use std::{collections::HashMap, fmt::Write as _};

use document::{Assets, Unreadable};
use package::{Package, Relation};
use roxmltree::Node as Xml;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use text::{Fragment, Mark, Node};

use crate::{
    model::{Model, Source, schema},
    styles,
};

pub(crate) const MAIN_PART: &str = "word/document.xml";
const MAIN: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_MAIN: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_RELATIONSHIP: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const DRAWING_MAIN: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WORD_DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const EMU_PER_PIXEL: f64 = 9525.0;
const SHORT_HASH: usize = 16;

fn broken(error: impl std::fmt::Display) -> Unreadable {
    Unreadable(error.to_string())
}

pub(crate) fn is_word(node: Xml<'_, '_>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && matches!(node.tag_name().namespace(), Some(MAIN | STRICT_MAIN))
}

fn word_child<'a, 'input>(node: Xml<'a, 'input>, name: &str) -> Option<Xml<'a, 'input>> {
    node.children().find(|child| is_word(*child, name))
}

pub(crate) fn word_attribute<'a>(node: Xml<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| {
            attribute.name() == name && matches!(attribute.namespace(), Some(MAIN | STRICT_MAIN))
        })
        .map(|attribute| attribute.value())
}

fn relation_id<'a>(node: Xml<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| {
            attribute.name() == name
                && matches!(
                    attribute.namespace(),
                    Some(RELATIONSHIP | STRICT_RELATIONSHIP)
                )
        })
        .map(|attribute| attribute.value())
}

fn hash_of(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut hash = String::with_capacity(SHORT_HASH);
    for byte in &digest[..SHORT_HASH / 2] {
        let _ = write!(hash, "{byte:02x}");
    }
    hash
}

fn truth(value: Option<&str>) -> bool {
    !matches!(value, Some("0" | "false" | "off"))
}

/// The look of a run, which marks hold apart from its original XML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "each is one mark of a run")]
pub(crate) struct RunLook {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<String>,
    pub size: Option<String>,
    pub font: Option<String>,
    pub highlight: Option<String>,
}

pub(crate) fn run_look(properties: Xml<'_, '_>) -> RunLook {
    let on = |name: &str| {
        word_child(properties, name).is_some_and(|node| truth(word_attribute(node, "val")))
    };
    let value = |name: &str, attribute: &str| {
        word_child(properties, name)
            .and_then(|node| word_attribute(node, attribute))
            .map(str::to_owned)
    };
    RunLook {
        bold: on("b"),
        italic: on("i"),
        underline: word_child(properties, "u")
            .is_some_and(|node| word_attribute(node, "val") != Some("none")),
        strike: on("strike"),
        color: value("color", "val").filter(|color| color != "auto"),
        size: value("sz", "val"),
        font: value("rFonts", "ascii"),
        highlight: value("highlight", "val"),
    }
}

struct Reader<'t, 'input> {
    source: &'input str,
    relations: HashMap<String, Relation>,
    package: Package<'t>,
    assets: &'t mut Assets,
    raws: HashMap<String, String>,
    seen: HashMap<String, usize>,
    sources: HashMap<String, Source>,
}

pub(crate) fn read(file: &[u8], assets: &mut Assets) -> Result<Model, Unreadable> {
    let mut package = Package::open(file).map_err(broken)?;
    let xml = package
        .text(MAIN_PART)
        .ok_or_else(|| Unreadable("the file has no document part".to_owned()))?;
    let parsed = roxmltree::Document::parse(&xml).map_err(broken)?;
    let body = word_child(parsed.root_element(), "body")
        .ok_or_else(|| Unreadable("the document has no body".to_owned()))?;
    let relations = package.relations(MAIN_PART);
    let view = view_data(&mut package, body);
    let mut reader = Reader {
        source: &xml,
        relations,
        package,
        assets,
        raws: HashMap::new(),
        seen: HashMap::new(),
        sources: HashMap::new(),
    };
    let mut blocks = Vec::new();
    for child in body.children().filter(Xml::is_element) {
        if is_word(child, "sectPr") {
            continue;
        }
        blocks.push(reader.top_block(child).map_err(broken)?);
    }
    if blocks.is_empty() {
        blocks.push(empty_paragraph().map_err(broken)?);
    }
    let doc = Node::new(
        schema(),
        schema().top_node(),
        None,
        Fragment::from_array(blocks),
        Vec::new(),
    )
    .map_err(broken)?;
    let (raws, sources) = (reader.raws, reader.sources);
    Ok(Model {
        doc,
        sources,
        raws,
        view,
    })
}

fn view_data(package: &mut Package<'_>, body: Xml<'_, '_>) -> Value {
    let styles = package
        .text("word/styles.xml")
        .map_or(Value::Null, |text| styles::styles(&text));
    let numbering = package
        .text("word/numbering.xml")
        .map_or(Value::Null, |text| styles::numbering(&text));
    let section = word_child(body, "sectPr");
    let page = section.map_or(Value::Null, styles::page);
    json!({ "styles": styles, "numbering": numbering, "page": page })
}

pub(crate) fn empty_paragraph() -> Result<Node, text::Failed> {
    let kind = schema().node_id("paragraph").unwrap_or_default();
    Node::new(schema(), kind, None, Fragment::default(), Vec::new())
}

fn mark(name: &str, attrs: Option<&Value>) -> Result<Mark, text::Failed> {
    let kind = schema().mark_id(name).unwrap_or_default();
    Mark::new(schema(), kind, attrs)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "each call builds its attributes here"
)]
fn node(name: &str, attrs: Value, content: Vec<Node>) -> Result<Node, text::Failed> {
    let kind = schema().node_id(name).unwrap_or_default();
    Node::new(
        schema(),
        kind,
        Some(&attrs),
        Fragment::from_array(content),
        Vec::new(),
    )
}

impl<'input> Reader<'_, 'input> {
    fn xml_of(&self, node: Xml<'_, 'input>) -> &'input str {
        &self.source[node.range()]
    }

    /// Keeps a piece of the original XML, and gives the name that attributes hold.
    fn keep(&mut self, xml: &str) -> String {
        let hash = hash_of(xml);
        self.raws
            .entry(hash.clone())
            .or_insert_with(|| xml.to_owned());
        hash
    }

    fn keep_child(&mut self, parent: Xml<'_, 'input>, name: &str) -> Option<String> {
        word_child(parent, name).map(|child| self.keep(self.xml_of(child)))
    }

    /// Gives a top-level block an id that stays the same for the same XML, and only one block
    /// in the document holds it.
    fn next_id(&mut self, xml: &str) -> String {
        let hash = hash_of(xml);
        let count = self.seen.entry(hash.clone()).or_insert(0);
        *count += 1;
        format!("{hash}-{count}")
    }

    fn top_block(&mut self, element: Xml<'_, 'input>) -> Result<Node, text::Failed> {
        let xml = self.xml_of(element);
        let id = self.next_id(xml);
        let block = self.block(element, Some(&id))?;
        self.sources.insert(
            id,
            Source {
                node: block.clone(),
                xml: xml.to_owned(),
            },
        );
        Ok(block)
    }

    fn block(&mut self, element: Xml<'_, 'input>, id: Option<&str>) -> Result<Node, text::Failed> {
        if is_word(element, "p") {
            self.paragraph(element, id)
        } else if is_word(element, "tbl") && simple_table(element) {
            self.table(element, id)
        } else {
            let raw = self.keep(self.xml_of(element));
            node(
                "opaque",
                json!({ "id": id, "raw": raw, "text": all_text(element) }),
                Vec::new(),
            )
        }
    }

    fn paragraph(
        &mut self,
        element: Xml<'_, 'input>,
        id: Option<&str>,
    ) -> Result<Node, text::Failed> {
        let properties = word_child(element, "pPr");
        let value = |name: &str| {
            properties
                .and_then(|node| word_child(node, name))
                .and_then(|node| word_attribute(node, "val"))
        };
        let number = properties.and_then(|node| word_child(node, "numPr"));
        let number_value = |name: &str| {
            number
                .and_then(|node| word_child(node, name))
                .and_then(|node| word_attribute(node, "val"))
        };
        let attrs = json!({
            "id": id,
            "style": value("pStyle"),
            "align": value("jc"),
            "numId": number_value("numId"),
            "ilvl": number_value("ilvl"),
            "props": self.keep_child(element, "pPr"),
        });
        let mut inlines = Vec::new();
        for child in element.children().filter(Xml::is_element) {
            if !is_word(child, "pPr") {
                self.inline(child, &[], &mut inlines)?;
            }
        }
        node("paragraph", attrs, inlines)
    }

    fn inline(
        &mut self,
        element: Xml<'_, 'input>,
        marks: &[Mark],
        out: &mut Vec<Node>,
    ) -> Result<(), text::Failed> {
        if is_word(element, "r") {
            return self.run(element, marks, out);
        }
        let wrapper = if is_word(element, "hyperlink") {
            self.link_mark(element)?
        } else if is_word(element, "ins") {
            Some(change_mark("insertion", element)?)
        } else if is_word(element, "del") {
            Some(change_mark("deletion", element)?)
        } else {
            None
        };
        let Some(wrapper) = wrapper else {
            return self.opaque_inline(element, out);
        };
        let mut inner = marks.to_vec();
        inner.push(wrapper);
        for child in element.children().filter(Xml::is_element) {
            self.inline(child, &inner, out)?;
        }
        Ok(())
    }

    fn link_mark(&self, element: Xml<'_, 'input>) -> Result<Option<Mark>, text::Failed> {
        let Some(id) = relation_id(element, "id") else {
            return Ok(None);
        };
        let Some(relation) = self.relations.get(id) else {
            return Ok(None);
        };
        mark(
            "link",
            Some(&json!({ "href": relation.target, "relation": id })),
        )
        .map(Some)
    }

    fn opaque_inline(
        &mut self,
        element: Xml<'_, 'input>,
        out: &mut Vec<Node>,
    ) -> Result<(), text::Failed> {
        let raw = self.keep(self.xml_of(element));
        out.push(node(
            "inlineOpaque",
            json!({ "raw": raw, "text": all_text(element) }),
            Vec::new(),
        )?);
        Ok(())
    }

    fn run(
        &mut self,
        element: Xml<'_, 'input>,
        marks: &[Mark],
        out: &mut Vec<Node>,
    ) -> Result<(), text::Failed> {
        let properties = word_child(element, "rPr");
        let mut marks = marks.to_vec();
        if let Some(properties) = properties {
            let props = self.keep(self.xml_of(properties));
            marks.push(mark("run", Some(&json!({ "props": props })))?);
            marks.extend(look_marks(&run_look(properties))?);
        }
        for child in element.children().filter(Xml::is_element) {
            if is_word(child, "rPr") {
                continue;
            }
            if is_word(child, "t") || is_word(child, "delText") {
                if let Some(text) = child.text().filter(|text| !text.is_empty()) {
                    out.push(Node::text(schema(), text, marks.clone())?);
                }
            } else if is_word(child, "tab") {
                out.push(with_marks("tab", &marks)?);
            } else if is_word(child, "br") && word_attribute(child, "type").is_none() {
                out.push(with_marks("hardBreak", &marks)?);
            } else if is_word(child, "drawing") {
                out.push(self.drawing(element, child, &marks)?);
            } else {
                let raw = self.keep(self.xml_of(element));
                out.push(node(
                    "inlineOpaque",
                    json!({ "raw": raw, "text": all_text(element) }),
                    Vec::new(),
                )?);
                return Ok(());
            }
        }
        Ok(())
    }

    fn drawing(
        &mut self,
        run: Xml<'_, 'input>,
        drawing: Xml<'_, 'input>,
        marks: &[Mark],
    ) -> Result<Node, text::Failed> {
        let raw = self.keep(self.xml_of(run));
        let picture = drawing
            .descendants()
            .find(|node| node.has_tag_name((DRAWING_MAIN, "blip")))
            .and_then(|blip| relation_id(blip, "embed"))
            .and_then(|id| self.relations.get(id))
            .map(|relation| relation.target.clone())
            .and_then(|target| self.package.bytes(&target));
        let extent = drawing
            .descendants()
            .find(|node| node.has_tag_name((WORD_DRAWING, "extent")));
        let pixels = |name: &str| {
            extent
                .and_then(|node| node.attribute(name))
                .and_then(|value| value.parse::<f64>().ok())
                .map_or(0.0, |value| (value / EMU_PER_PIXEL).round())
        };
        let Some(bytes) = picture else {
            return node(
                "inlineOpaque",
                json!({ "raw": raw, "text": all_text(run) }),
                Vec::new(),
            );
        };
        let hash = self.assets.put(&bytes);
        let asset = hash.iter().fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        });
        let kind = schema().node_id("image").unwrap_or_default();
        Node::new(
            schema(),
            kind,
            Some(&json!({
                "asset": asset,
                "width": pixels("cx"),
                "height": pixels("cy"),
                "raw": raw,
            })),
            Fragment::default(),
            marks.to_vec(),
        )
    }

    fn table(&mut self, element: Xml<'_, 'input>, id: Option<&str>) -> Result<Node, text::Failed> {
        let table_props = word_child(element, "tblPr").map(|node| self.xml_of(node));
        let grid = word_child(element, "tblGrid").map(|node| self.xml_of(node));
        let joined = [table_props, grid]
            .into_iter()
            .flatten()
            .collect::<String>();
        let props = self.keep(&joined);
        let mut rows = Vec::new();
        for row in element.children().filter(|child| is_word(*child, "tr")) {
            let mut cells = Vec::new();
            for cell in row.children().filter(|child| is_word(*child, "tc")) {
                let properties = word_child(cell, "tcPr");
                let span = properties
                    .and_then(|node| word_child(node, "gridSpan"))
                    .and_then(|node| word_attribute(node, "val"))
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(1);
                let merged = properties
                    .and_then(|node| word_child(node, "vMerge"))
                    .is_some_and(|node| word_attribute(node, "val") != Some("restart"));
                let mut blocks = Vec::new();
                for child in cell.children().filter(Xml::is_element) {
                    if !is_word(child, "tcPr") {
                        blocks.push(self.block(child, None)?);
                    }
                }
                if blocks.is_empty() {
                    blocks.push(empty_paragraph()?);
                }
                cells.push(node(
                    "tableCell",
                    json!({
                        "props": self.keep_child(cell, "tcPr"),
                        "span": span,
                        "merged": merged,
                    }),
                    blocks,
                )?);
            }
            rows.push(node(
                "tableRow",
                json!({ "props": self.keep_child(row, "trPr") }),
                cells,
            )?);
        }
        node("table", json!({ "id": id, "props": props }), rows)
    }
}

fn simple_table(element: Xml<'_, '_>) -> bool {
    let mut rows = 0;
    for child in element.children().filter(Xml::is_element) {
        if is_word(child, "tr") {
            rows += 1;
            let mut cells = 0;
            for cell in child.children().filter(Xml::is_element) {
                if is_word(cell, "tc") {
                    cells += 1;
                } else if !is_word(cell, "trPr") && !is_word(cell, "tblPrEx") {
                    return false;
                }
            }
            if cells == 0 {
                return false;
            }
        } else if !is_word(child, "tblPr") && !is_word(child, "tblGrid") {
            return false;
        }
    }
    rows > 0
}

fn with_marks(name: &str, marks: &[Mark]) -> Result<Node, text::Failed> {
    let kind = schema().node_id(name).unwrap_or_default();
    Node::new(schema(), kind, None, Fragment::default(), marks.to_vec())
}

fn change_mark(name: &str, element: Xml<'_, '_>) -> Result<Mark, text::Failed> {
    mark(
        name,
        Some(&json!({
            "author": word_attribute(element, "author").unwrap_or_default(),
            "date": word_attribute(element, "date").unwrap_or_default(),
        })),
    )
}

pub(crate) fn look_marks(look: &RunLook) -> Result<Vec<Mark>, text::Failed> {
    let mut marks = Vec::new();
    for (on, name) in [
        (look.bold, "bold"),
        (look.italic, "italic"),
        (look.underline, "underline"),
        (look.strike, "strike"),
    ] {
        if on {
            marks.push(mark(name, None)?);
        }
    }
    if look.color.is_some()
        || look.size.is_some()
        || look.font.is_some()
        || look.highlight.is_some()
    {
        marks.push(mark(
            "textStyle",
            Some(&json!({
                "color": look.color,
                "size": look.size,
                "font": look.font,
                "highlight": look.highlight,
            })),
        )?);
    }
    Ok(marks)
}

fn all_text(element: Xml<'_, '_>) -> String {
    element
        .descendants()
        .filter(Xml::is_text)
        .filter_map(|node| node.text())
        .collect()
}
