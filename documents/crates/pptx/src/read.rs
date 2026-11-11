use std::{collections::HashMap, fmt::Write as _};

use document::{Assets, Unreadable};
use package::{Package, RELATIONSHIPS, Relation, STRICT_RELATIONSHIPS};
use roxmltree::{Document, Node as Xml};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use text::{Fragment, Mark, Node};

use crate::model::{Model, SHAPE_IDS, Shape, Slide, number_in, schema};

pub(crate) const PRESENTATION: &str = "ppt/presentation.xml";
const LAYOUT_KIND: &str = "slideLayout";
const MASTER_KIND: &str = "slideMaster";
const SHORT_HASH: usize = 16;
const TEXT_LEVELS: usize = 9;
const WHOLE_SCALE: f64 = 100_000.0;

fn broken(error: impl std::fmt::Display) -> Unreadable {
    Unreadable(error.to_string())
}

pub(crate) fn local<'a, 'input>(node: Xml<'a, 'input>, name: &str) -> Option<Xml<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn descendant<'a, 'input>(node: Xml<'a, 'input>, name: &str) -> Option<Xml<'a, 'input>> {
    node.descendants()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn relation_id<'a>(node: Xml<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| {
            attribute.name() == name
                && matches!(
                    attribute.namespace(),
                    Some(RELATIONSHIPS | STRICT_RELATIONSHIPS)
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Frame {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub rotation: i64,
}

pub(crate) fn frame_of(properties: Option<Xml<'_, '_>>) -> Option<Frame> {
    let transform = properties.and_then(|node| local(node, "xfrm"))?;
    let number = |node: Option<Xml<'_, '_>>, name: &str| {
        node.and_then(|node| node.attribute(name))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    };
    let offset = local(transform, "off");
    let extent = local(transform, "ext");
    Some(Frame {
        x: number(offset, "x"),
        y: number(offset, "y"),
        width: number(extent, "cx"),
        height: number(extent, "cy"),
        rotation: transform
            .attribute("rot")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
    })
}

fn color_of(fill: Option<Xml<'_, '_>>) -> Option<String> {
    let solid = fill.and_then(|node| local(node, "solidFill"))?;
    if let Some(color) = local(solid, "srgbClr") {
        return color.attribute("val").map(str::to_owned);
    }
    local(solid, "schemeClr")
        .and_then(|color| color.attribute("val"))
        .map(|name| format!("scheme:{name}"))
}

fn placeholder_key(shape: Xml<'_, '_>) -> Option<String> {
    let placeholder = descendant(local(shape, "nvSpPr")?, "ph")?;
    let kind = placeholder.attribute("type").unwrap_or("body");
    let index = placeholder.attribute("idx").unwrap_or("0");
    Some(format!("{kind}:{index}"))
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

fn mark(name: &str, attrs: Option<&Value>) -> Result<Mark, text::Failed> {
    let kind = schema().mark_id(name).unwrap_or_default();
    Mark::new(schema(), kind, attrs)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RunLook {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<String>,
    pub size: Option<String>,
    pub font: Option<String>,
}

pub(crate) fn run_look(properties: Xml<'_, '_>) -> RunLook {
    let on = |name: &str| matches!(properties.attribute(name), Some("1" | "true"));
    RunLook {
        bold: on("b"),
        italic: on("i"),
        underline: properties
            .attribute("u")
            .is_some_and(|value| value != "none"),
        color: color_of(Some(properties)),
        size: properties.attribute("sz").map(str::to_owned),
        font: local(properties, "latin")
            .and_then(|font| font.attribute("typeface"))
            .map(str::to_owned),
    }
}

fn look_marks(look: &RunLook) -> Result<Vec<Mark>, text::Failed> {
    let mut marks = Vec::new();
    for (on, name) in [
        (look.bold, "bold"),
        (look.italic, "italic"),
        (look.underline, "underline"),
    ] {
        if on {
            marks.push(mark(name, None)?);
        }
    }
    if look.color.is_some() || look.size.is_some() || look.font.is_some() {
        marks.push(mark(
            "textStyle",
            Some(&json!({ "color": look.color, "size": look.size, "font": look.font })),
        )?);
    }
    Ok(marks)
}

/// One look for each of the nine text levels. What a list does not set stays as it was.
type Levels = Vec<Map<String, Value>>;

fn merge_levels(levels: &mut Levels, list: Xml<'_, '_>) {
    levels.resize(TEXT_LEVELS, Map::new());
    for (index, level) in levels.iter_mut().enumerate() {
        let Some(properties) = local(list, &format!("lvl{}pPr", index + 1)) else {
            continue;
        };
        if let Some(align) = properties.attribute("algn") {
            level.insert("align".to_owned(), json!(align));
        }
        if let Some(margin) = properties.attribute("marL") {
            level.insert(
                "margin".to_owned(),
                json!(margin.parse::<i64>().unwrap_or(0)),
            );
        }
        if local(properties, "buNone").is_some() {
            level.insert("bullet".to_owned(), json!(false));
        } else if local(properties, "buChar").is_some() || local(properties, "buAutoNum").is_some()
        {
            level.insert("bullet".to_owned(), json!(true));
        }
        let Some(run) = local(properties, "defRPr") else {
            continue;
        };
        let look = run_look(run);
        for (name, value) in [("bold", run.attribute("b")), ("italic", run.attribute("i"))] {
            if let Some(value) = value {
                level.insert(name.to_owned(), json!(matches!(value, "1" | "true")));
            }
        }
        for (name, value) in [
            ("size", look.size),
            ("color", look.color),
            ("font", look.font),
        ] {
            if let Some(value) = value {
                level.insert(name.to_owned(), json!(value));
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Placeholder {
    frame: Option<Frame>,
    levels: Levels,
    anchor: Option<String>,
}

#[derive(Debug, Default)]
struct Inherited {
    master: HashMap<String, Placeholder>,
    layout: HashMap<String, Placeholder>,
    title: Levels,
    body: Levels,
    other: Levels,
}

impl Inherited {
    fn frame(&self, key: &str) -> Option<Frame> {
        let frame = |list: &HashMap<String, Placeholder>| list.get(key).and_then(|hold| hold.frame);
        frame(&self.layout).or_else(|| frame(&self.master))
    }

    /// A placeholder matches by its type in the master, where its index often differs.
    fn in_master(&self, key: &str) -> Option<&Placeholder> {
        let kind = key.split(':').next().unwrap_or(key);
        self.master.get(key).or_else(|| {
            self.master
                .iter()
                .find(|(other, _)| other.split(':').next() == Some(kind))
                .map(|(_, placeholder)| placeholder)
        })
    }

    fn text_look(&self, key: Option<&str>, body: Option<Xml<'_, '_>>) -> Value {
        let kind = key.and_then(|key| key.split(':').next());
        let mut levels = match kind {
            Some("title" | "ctrTitle") => self.title.clone(),
            Some("dt" | "ftr" | "sldNum") | None => self.other.clone(),
            Some(_) => self.body.clone(),
        };
        levels.resize(TEXT_LEVELS, Map::new());
        let mut anchor = None;
        let held = key.map_or([None, None], |key| {
            [self.in_master(key), self.layout.get(key)]
        });
        for placeholder in held.into_iter().flatten() {
            for (target, level) in levels.iter_mut().zip(&placeholder.levels) {
                target.extend(level.clone());
            }
            anchor = placeholder.anchor.clone().or(anchor);
        }
        if let Some(list) = body.and_then(|body| local(body, "lstStyle")) {
            merge_levels(&mut levels, list);
        }
        let properties = body.and_then(|body| local(body, "bodyPr"));
        let scale = properties
            .and_then(|node| local(node, "normAutofit"))
            .and_then(|fit| fit.attribute("fontScale"))
            .and_then(|scale| scale.parse::<f64>().ok())
            .map(|scale| scale / WHOLE_SCALE);
        json!({
            "levels": levels,
            "anchor": properties.and_then(|node| node.attribute("anchor")).map(str::to_owned).or(anchor),
            "scale": scale,
        })
    }
}

struct Reader<'p, 'file, 'input> {
    source: &'input str,
    raws: &'p mut HashMap<String, String>,
    relations: &'p HashMap<String, Relation>,
    slides: &'p HashMap<String, String>,
    package: &'p mut Package<'file>,
    assets: &'p mut Assets,
}

impl<'input> Reader<'_, '_, 'input> {
    fn xml_of(&self, node: Xml<'_, 'input>) -> &'input str {
        &self.source[node.range()]
    }

    fn keep(&mut self, xml: &str) -> String {
        let hash = hash_of(xml);
        self.raws
            .entry(hash.clone())
            .or_insert_with(|| xml.to_owned());
        hash
    }

    fn text_tree(&mut self, body: Xml<'_, 'input>) -> Result<Node, text::Failed> {
        let mut paragraphs = Vec::new();
        for paragraph in body
            .children()
            .filter(|child| child.is_element() && child.tag_name().name() == "p")
        {
            paragraphs.push(self.paragraph(paragraph)?);
        }
        if paragraphs.is_empty() {
            paragraphs.push(node("paragraph", json!({}), Vec::new())?);
        }
        node("doc", json!({}), paragraphs)
    }

    fn paragraph(&mut self, paragraph: Xml<'_, 'input>) -> Result<Node, text::Failed> {
        let properties = local(paragraph, "pPr");
        let bullet = properties.is_some_and(|node| {
            local(node, "buChar").is_some() || local(node, "buAutoNum").is_some()
        });
        let attrs = json!({
            "align": properties.and_then(|node| node.attribute("algn")),
            "level": properties
                .and_then(|node| node.attribute("lvl"))
                .and_then(|level| level.parse::<u32>().ok())
                .unwrap_or(0),
            "bullet": bullet,
            "props": properties.map(|node| self.keep(self.xml_of(node))),
            "end": local(paragraph, "endParaRPr").map(|node| self.keep(self.xml_of(node))),
        });
        let mut inlines = Vec::new();
        for child in paragraph.children().filter(Xml::is_element) {
            match child.tag_name().name() {
                "pPr" | "endParaRPr" => {}
                "r" => self.run(child, &mut inlines)?,
                "br" => {
                    let kind = schema().node_id("hardBreak").unwrap_or_default();
                    inlines.push(Node::new(
                        schema(),
                        kind,
                        None,
                        Fragment::default(),
                        Vec::new(),
                    )?);
                }
                _ => {
                    let raw = self.keep(self.xml_of(child));
                    let text: String = child
                        .descendants()
                        .filter(Xml::is_text)
                        .filter_map(|node| node.text())
                        .collect();
                    inlines.push(node(
                        "inlineOpaque",
                        json!({ "raw": raw, "text": text }),
                        Vec::new(),
                    )?);
                }
            }
        }
        node("paragraph", attrs, inlines)
    }

    fn run(&mut self, run: Xml<'_, 'input>, out: &mut Vec<Node>) -> Result<(), text::Failed> {
        let mut marks = Vec::new();
        if let Some(properties) = local(run, "rPr") {
            let props = self.keep(self.xml_of(properties));
            marks.push(mark("run", Some(&json!({ "props": props })))?);
            marks.extend(look_marks(&run_look(properties))?);
            if let Some(link) =
                local(properties, "hlinkClick").and_then(|click| self.link_of(click))
            {
                marks.push(mark("link", Some(&link))?);
            }
        }
        if let Some(text) = local(run, "t")
            .and_then(|text| text.text())
            .filter(|text| !text.is_empty())
        {
            out.push(Node::text(schema(), text, marks)?);
        }
        Ok(())
    }

    fn shape(
        &mut self,
        element: Xml<'_, 'input>,
        order: u64,
        inherited: &Inherited,
    ) -> Result<Option<Shape>, text::Failed> {
        let kind = match element.tag_name().name() {
            "sp" => "shape",
            "pic" => "picture",
            "graphicFrame" => "graphic",
            "grpSp" => "group",
            "cxnSp" => "connector",
            _ => return Ok(None),
        };
        let names = element
            .children()
            .filter(Xml::is_element)
            .find(|child| child.tag_name().name().starts_with("nv"))
            .and_then(|names| descendant(names, "cNvPr"));
        let Some(names) = names else {
            return Ok(None);
        };
        let properties = local(element, "spPr").or_else(|| local(element, "grpSpPr"));
        let placeholder = placeholder_key(element);
        let frame = frame_of(properties)
            .or_else(|| frame_of(Some(element)))
            .or_else(|| placeholder.as_ref().and_then(|key| inherited.frame(key)))
            .unwrap_or_default();
        let body = local(element, "txBody");
        let text = match body {
            Some(body) => Some(self.text_tree(body)?),
            None => None,
        };
        let look = body.map_or(Value::Null, |body| {
            inherited.text_look(placeholder.as_deref(), Some(body))
        });
        let asset = self.picture(element);
        let link = local(names, "hlinkClick").and_then(|click| self.link_of(click));
        Ok(Some(Shape {
            id: names.attribute("id").unwrap_or_default().to_owned(),
            name: names.attribute("name").unwrap_or_default().to_owned(),
            kind: kind.to_owned(),
            placeholder,
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
            rotation: frame.rotation,
            geometry: properties
                .and_then(|node| local(node, "prstGeom"))
                .and_then(|node| node.attribute("prst"))
                .map(str::to_owned),
            fill: color_of(properties),
            line: properties.and_then(|node| color_of(local(node, "ln"))),
            asset,
            link,
            text,
            look,
            order,
        }))
    }

    /// The run or shape keeps its click in its own XML, so a save keeps it.
    fn link_of(&self, click: Xml<'_, '_>) -> Option<Value> {
        let action = click.attribute("action").unwrap_or_default();
        if let Some(jump) = action.strip_prefix("ppaction://hlinkshowjump?jump=") {
            return Some(json!({ "jump": jump }));
        }
        let relation = self.relations.get(relation_id(click, "id")?)?;
        if action.starts_with("ppaction://hlinksldjump") {
            return self
                .slides
                .get(&relation.target)
                .map(|slide| json!({ "slide": slide }));
        }
        (relation.external || relation.kind.ends_with("/hyperlink"))
            .then(|| json!({ "href": relation.target }))
    }

    fn picture(&mut self, element: Xml<'_, 'input>) -> Option<String> {
        let blip = descendant(local(element, "blipFill")?, "blip")?;
        let target = self
            .relations
            .get(relation_id(blip, "embed")?)?
            .target
            .clone();
        let bytes = self.package.bytes(&target)?;
        Some(hex(&self.assets.put(&bytes)))
    }

    fn shapes(
        &mut self,
        root: Xml<'_, 'input>,
        inherited: &Inherited,
    ) -> Result<Vec<Shape>, text::Failed> {
        let Some(tree) = descendant(root, "spTree") else {
            return Ok(Vec::new());
        };
        let mut shapes = Vec::new();
        for element in tree.children().filter(Xml::is_element) {
            let order = shapes.len() as u64;
            if let Some(shape) = self.shape(element, order, inherited)? {
                shapes.push(shape);
            }
        }
        Ok(shapes)
    }
}

fn placeholders(xml: &str) -> HashMap<String, Placeholder> {
    let Ok(parsed) = Document::parse(xml) else {
        return HashMap::new();
    };
    parsed
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "sp")
        .filter_map(|shape| {
            let body = local(shape, "txBody");
            let mut levels = Levels::new();
            if let Some(list) = body.and_then(|body| local(body, "lstStyle")) {
                merge_levels(&mut levels, list);
            }
            let placeholder = Placeholder {
                frame: frame_of(local(shape, "spPr")),
                levels,
                anchor: body
                    .and_then(|body| local(body, "bodyPr"))
                    .and_then(|properties| properties.attribute("anchor"))
                    .map(str::to_owned),
            };
            Some((placeholder_key(shape)?, placeholder))
        })
        .collect()
}

fn text_styles(xml: &str) -> [Levels; 3] {
    let mut styles = [Levels::new(), Levels::new(), Levels::new()];
    let Ok(parsed) = Document::parse(xml) else {
        return styles;
    };
    if let Some(list) = descendant(parsed.root_element(), "txStyles") {
        for (levels, name) in styles
            .iter_mut()
            .zip(["titleStyle", "bodyStyle", "otherStyle"])
        {
            if let Some(style) = local(list, name) {
                merge_levels(levels, style);
            }
        }
    }
    styles
}

fn background_of(root: Xml<'_, '_>) -> Option<String> {
    let background = descendant(root, "bgPr")?;
    color_of(Some(background))
}

fn theme_of(package: &mut Package<'_>, master: Option<&str>) -> Value {
    let Some(master) = master else {
        return Value::Null;
    };
    let theme = package
        .relations(master)
        .into_values()
        .find(|relation| relation.kind.ends_with("/theme"));
    let Some(xml) = theme.and_then(|relation| package.text(&relation.target)) else {
        return Value::Null;
    };
    let Ok(parsed) = Document::parse(&xml) else {
        return Value::Null;
    };
    let Some(scheme) = descendant(parsed.root_element(), "clrScheme") else {
        return Value::Null;
    };
    let mut theme: Map<String, Value> = scheme
        .children()
        .filter(Xml::is_element)
        .filter_map(|color| {
            let value = color.first_element_child()?;
            let hex = value
                .attribute("val")
                .filter(|_| value.tag_name().name() == "srgbClr")
                .or_else(|| value.attribute("lastClr"))?;
            Some((color.tag_name().name().to_owned(), json!(hex)))
        })
        .collect();
    if let Some(fonts) = descendant(parsed.root_element(), "fontScheme") {
        for (part, name) in [("majorFont", "+mj-lt"), ("minorFont", "+mn-lt")] {
            let face = local(fonts, part)
                .and_then(|font| local(font, "latin"))
                .and_then(|latin| latin.attribute("typeface"));
            if let Some(face) = face {
                theme.insert(name.to_owned(), json!(face));
            }
        }
    }
    Value::Object(theme)
}

fn linked_part(relations: &HashMap<String, Relation>, kind: &str) -> Option<String> {
    relations
        .values()
        .find(|relation| relation.kind.ends_with(kind))
        .map(|relation| relation.target.clone())
}

fn slide_ids(
    entries: &[Xml<'_, '_>],
    relations: &HashMap<String, Relation>,
) -> HashMap<String, String> {
    entries
        .iter()
        .filter_map(|entry| {
            let relation = relations.get(relation_id(*entry, "id")?)?;
            Some((relation.target.clone(), entry.attribute("id")?.to_owned()))
        })
        .collect()
}

fn last_shape(root: Xml<'_, '_>) -> u64 {
    root.descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "cNvPr")
        .filter_map(|node| number_in(node.attribute("id")?, &SHAPE_IDS))
        .max()
        .unwrap_or(0)
}

fn inherited(master: Option<&str>, layout: Option<&str>) -> Inherited {
    let [title, body, other] = master.map(text_styles).unwrap_or_default();
    Inherited {
        master: master.map(placeholders).unwrap_or_default(),
        layout: layout.map(placeholders).unwrap_or_default(),
        title,
        body,
        other,
    }
}

pub(crate) fn read(file: &[u8], assets: &mut Assets) -> Result<Model, Unreadable> {
    let mut package = Package::open(file).map_err(broken)?;
    let presentation_xml = package
        .text(PRESENTATION)
        .ok_or_else(|| Unreadable("the file has no presentation".to_owned()))?;
    let relations = package.relations(PRESENTATION);
    let presentation = Document::parse(&presentation_xml).map_err(broken)?;
    let root = presentation.root_element();
    let size = local(root, "sldSz");
    let number = |name: &str| {
        size.and_then(|size| size.attribute(name))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    };
    let mut model = Model {
        width: number("cx"),
        height: number("cy"),
        ..Model::default()
    };
    let list = local(root, "sldIdLst");
    let entries: Vec<Xml<'_, '_>> = list
        .iter()
        .flat_map(Xml::children)
        .filter(Xml::is_element)
        .collect();
    let slide_ids = slide_ids(&entries, &relations);
    let mut first_master = None;
    for (index, entry) in entries.into_iter().enumerate() {
        let part = relation_id(entry, "id")
            .and_then(|id| relations.get(id))
            .map(|relation| relation.target.clone())
            .ok_or_else(|| Unreadable("a slide has no part".to_owned()))?;
        let xml = package
            .text(&part)
            .ok_or_else(|| Unreadable(format!("the part {part} is missing")))?;
        let parsed = Document::parse(&xml).map_err(broken)?;
        let slide_relations = package.relations(&part);
        let layout = linked_part(&slide_relations, LAYOUT_KIND);
        let layout_xml = layout.as_ref().and_then(|name| package.text(name));
        let layout_relations = layout
            .as_ref()
            .map(|name| package.relations(name))
            .unwrap_or_default();
        let master = linked_part(&layout_relations, MASTER_KIND);
        let master_xml = master.as_ref().and_then(|name| package.text(name));
        if first_master.is_none() {
            first_master.clone_from(&master);
        }
        let inherited = inherited(master_xml.as_deref(), layout_xml.as_deref());
        let mut reader = Reader {
            source: &xml,
            raws: &mut model.raws,
            relations: &slide_relations,
            slides: &slide_ids,
            package: &mut package,
            assets,
        };
        let shapes = reader
            .shapes(parsed.root_element(), &inherited)
            .map_err(broken)?;
        let background = background_of(parsed.root_element());
        let mut behind = Vec::new();
        for (source, relations) in [
            (master_xml.as_deref(), None),
            (layout_xml.as_deref(), Some(&layout_relations)),
        ] {
            let Some(source) = source else {
                continue;
            };
            let parsed = Document::parse(source).map_err(broken)?;
            let empty = HashMap::new();
            let mut reader = Reader {
                source,
                raws: &mut model.raws,
                relations: relations.unwrap_or(&empty),
                slides: &slide_ids,
                package: &mut package,
                assets,
            };
            let drawn = reader
                .shapes(parsed.root_element(), &inherited)
                .map_err(broken)?;
            behind.extend(
                drawn
                    .into_iter()
                    .filter(|shape| shape.placeholder.is_none()),
            );
        }
        model.slides.push(Slide {
            id: entry.attribute("id").unwrap_or_default().to_owned(),
            part: Some(part),
            position: f64::from(u32::try_from(index).unwrap_or(0)),
            shapes,
            behind,
            background,
            text_look: inherited.text_look(None, None),
            last_shape: last_shape(parsed.root_element()),
        });
    }
    model.theme = theme_of(&mut package, first_master.as_deref());
    Ok(model)
}
