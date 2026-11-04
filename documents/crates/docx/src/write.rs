use std::{collections::HashSet, fmt::Write as _};

use document::{Assets, Hash, Unwritable};
use package::{Package, Parts, RELATIONSHIPS, escape, rewrite};
use roxmltree::Node as Xml;
use serde_json::Value;
use text::{Mark, Node};

use crate::{
    model::{Model, schema},
    read::{MAIN_PART, RunLook, is_word, run_look, word_attribute},
};

const RELATIONS_PART: &str = "word/_rels/document.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";
const HYPERLINK_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const IMAGE_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const FIRST_CHANGE_ID: u32 = 90_000;
const EMU_PER_PIXEL: f64 = 9525.0;

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
    links: Vec<(String, String)>,
    images: Vec<(String, Hash, String)>,
    next_relation: u32,
    next_change: u32,
    next_picture: u32,
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
        links: Vec::new(),
        images: Vec::new(),
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
        let wrapped = format!("<piece{}>{xml}</piece>", self.namespaces);
        let parsed = roxmltree::Document::parse(&wrapped).map_err(|error| error.to_string())?;
        let piece = parsed
            .root_element()
            .first_element_child()
            .ok_or_else(|| "an empty piece".to_owned())?;
        Ok(read(piece))
    }

    fn paragraph_props(&self, node: &Node) -> Result<String, String> {
        let raw = match text_attr(node, "props") {
            Some(name) => self.raw(name)?.to_owned(),
            None => String::new(),
        };
        let wanted = |name: &str| text_attr(node, name).map(str::to_owned);
        let (style, align, list, level) = (
            wanted("style"),
            wanted("align"),
            wanted("numId"),
            wanted("ilvl"),
        );
        if raw.is_empty() {
            let mut props = String::new();
            add_paragraph_props(
                &mut props,
                style.as_ref(),
                align.as_ref(),
                list.as_ref(),
                level.as_ref(),
            );
            return Ok(if props.is_empty() {
                props
            } else {
                format!("<w:pPr>{props}</w:pPr>")
            });
        }
        self.parse_piece(&raw, |properties| {
            let value = |name: &str| {
                properties
                    .children()
                    .find(|child| is_word(*child, name))
                    .and_then(|child| word_attribute(child, "val"))
                    .map(str::to_owned)
            };
            let number = properties.children().find(|child| is_word(*child, "numPr"));
            let number_value = |name: &str| {
                number
                    .and_then(|node| node.children().find(|child| is_word(*child, name)))
                    .and_then(|child| word_attribute(child, "val"))
                    .map(str::to_owned)
            };
            let same = value("pStyle") == style
                && value("jc") == align
                && number_value("numId") == list
                && number_value("ilvl") == level;
            if same {
                return raw.clone();
            }
            let mut children: Vec<(usize, String)> = properties
                .children()
                .filter(Xml::is_element)
                .filter(|child| {
                    !["pStyle", "jc", "numPr"]
                        .iter()
                        .any(|name| is_word(*child, name))
                })
                .map(|child| {
                    let name = child.tag_name().name();
                    (
                        rank(PARAGRAPH_ORDER, name),
                        raw[shifted(properties, child)].to_owned(),
                    )
                })
                .collect();
            if let Some(style) = &style {
                children.push((
                    rank(PARAGRAPH_ORDER, "pStyle"),
                    format!(r#"<w:pStyle w:val="{}"/>"#, escape(style)),
                ));
            }
            if let Some(list) = &list {
                children.push((
                    rank(PARAGRAPH_ORDER, "numPr"),
                    format!(
                        r#"<w:numPr><w:ilvl w:val="{}"/><w:numId w:val="{}"/></w:numPr>"#,
                        escape(level.as_deref().unwrap_or("0")),
                        escape(list)
                    ),
                ));
            }
            if let Some(align) = &align {
                children.push((
                    rank(PARAGRAPH_ORDER, "jc"),
                    format!(r#"<w:jc w:val="{}"/>"#, escape(align)),
                ));
            }
            children.sort_by_key(|(rank, _)| *rank);
            let inner: String = children.into_iter().map(|(_, xml)| xml).collect();
            format!("<w:pPr>{inner}</w:pPr>")
        })
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
        let id = self.next_change;
        self.next_change += 1;
        format!(
            r#"<w:{tag} w:id="{id}" w:author="{}" w:date="{}">"#,
            escape(mark_text(mark, "author").unwrap_or_default()),
            escape(mark_text(mark, "date").unwrap_or_default()),
        )
    }

    fn link_open(&mut self, link: &Mark) -> String {
        if let Some(relation) = mark_text(link, "relation") {
            return format!(
                r#"<w:hyperlink xmlns:r="{RELATIONSHIPS}" r:id="{}">"#,
                escape(relation)
            );
        }
        let id = self.new_relation();
        let href = mark_text(link, "href").unwrap_or_default().to_owned();
        self.links.push((id.clone(), href));
        format!(r#"<w:hyperlink xmlns:r="{RELATIONSHIPS}" r:id="{id}">"#)
    }

    fn new_relation(&mut self) -> String {
        let id = format!("rIdDoc{}", self.next_relation);
        self.next_relation += 1;
        id
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
            return Ok(wrap_run_props(children));
        }
        self.parse_piece(&raw, |properties| {
            if run_look(properties) == wanted {
                return raw.clone();
            }
            let managed = ["b", "i", "u", "strike", "color", "sz", "highlight"];
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
            wrap_run_props(children)
        })
    }

    fn table(&mut self, node: &Node) -> Result<String, String> {
        let props = match text_attr(node, "props") {
            Some(name) => self.raw(name)?.to_owned(),
            None => "<w:tblPr/>".to_owned(),
        };
        let mut xml = format!("<w:tbl>{props}");
        for row in node.content().children() {
            xml.push_str("<w:tr>");
            if let Some(name) = text_attr(row, "props") {
                xml.push_str(self.raw(name)?);
            }
            for cell in row.content().children() {
                xml.push_str("<w:tc>");
                if let Some(name) = text_attr(cell, "props") {
                    xml.push_str(self.raw(name)?);
                }
                for block in cell.content().children() {
                    let inner = self.block(block)?;
                    xml.push_str(&inner);
                }
                xml.push_str("</w:tc>");
            }
            xml.push_str("</w:tr>");
        }
        xml.push_str("</w:tbl>");
        Ok(xml)
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
        let id = self.new_relation();
        self.images.push((id.clone(), hash, extension.to_owned()));
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
            let name: String = hash.iter().take(8).fold(String::new(), |mut text, byte| {
                let _ = write!(text, "{byte:02x}");
                text
            });
            let target = format!("media/{name}.{extension}");
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

fn add_paragraph_props(
    xml: &mut String,
    style: Option<&String>,
    align: Option<&String>,
    list: Option<&String>,
    level: Option<&String>,
) {
    if let Some(style) = style {
        let _ = write!(xml, r#"<w:pStyle w:val="{}"/>"#, escape(style));
    }
    if let Some(list) = list {
        let _ = write!(
            xml,
            r#"<w:numPr><w:ilvl w:val="{}"/><w:numId w:val="{}"/></w:numPr>"#,
            escape(level.map_or("0", String::as_str)),
            escape(list)
        );
    }
    if let Some(align) = align {
        let _ = write!(xml, r#"<w:jc w:val="{}"/>"#, escape(align));
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
}

fn wrap_run_props(mut children: Vec<(usize, String)>) -> String {
    if children.is_empty() {
        return String::new();
    }
    children.sort_by_key(|(rank, _)| *rank);
    let inner: String = children.into_iter().map(|(_, xml)| xml).collect();
    format!("<w:rPr>{inner}</w:rPr>")
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
