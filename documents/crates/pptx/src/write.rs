use std::{collections::HashMap, fmt::Write as _};

use document::{Assets, Unreadable, Unwritable};
use package::{Package, Parts, RELATIONSHIPS, escape, rewrite};
use roxmltree::{Document, Node as Xml};
use serde_json::Value;
use text::{Mark, Node};

use crate::{
    model::{Model, Shape, Slide, schema},
    read::{self, PRESENTATION, RunLook, local, run_look},
};

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PRESENTATION_MAIN: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const SLIDE_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const LAYOUT_KIND: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const SLIDE_TYPE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const PRESENTATION_RELATIONS: &str = "ppt/_rels/presentation.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";
/// The fills of a shape, of which its properties hold one at most.
const FILLS: [&str; 6] = [
    "noFill",
    "solidFill",
    "gradFill",
    "blipFill",
    "pattFill",
    "grpFill",
];

fn failed(error: impl std::fmt::Display) -> Unwritable {
    Unwritable(error.to_string())
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
        color: value("color"),
        size: value("size"),
        font: value("font"),
    }
}

fn fill_xml(color: &str) -> String {
    color.strip_prefix("scheme:").map_or_else(
        || {
            format!(
                r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#,
                escape(color)
            )
        },
        |name| {
            format!(
                r#"<a:solidFill><a:schemeClr val="{}"/></a:solidFill>"#,
                escape(name)
            )
        },
    )
}

/// Parses a piece of a slide, with the namespaces of that slide.
fn with_piece<T>(
    xml: &str,
    namespaces: &str,
    read: impl FnOnce(Xml<'_, '_>) -> T,
) -> Result<T, String> {
    let wrapped = format!("<piece{namespaces}>{xml}</piece>");
    let parsed = Document::parse(&wrapped).map_err(|error| error.to_string())?;
    let piece = parsed
        .root_element()
        .first_element_child()
        .ok_or("an empty piece")?;
    Ok(read(piece))
}

struct Out<'m> {
    raws: &'m HashMap<String, String>,
    namespaces: String,
}

impl Out<'_> {
    fn raw(&self, name: &str) -> Result<&str, String> {
        self.raws
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("the piece {name} is missing"))
    }

    fn run_props(&self, marks: &[Mark]) -> Result<String, String> {
        let wanted = look_of(marks);
        let raw = match mark_named(marks, "run").and_then(|mark| mark_text(mark, "props")) {
            Some(name) => self.raw(name)?.to_owned(),
            None => String::new(),
        };
        if raw.is_empty() {
            return Ok(new_run_props(&wanted, "a:rPr", ""));
        }
        with_piece(&raw, &self.namespaces, |properties| {
            if run_look(properties) == wanted {
                return raw.clone();
            }
            let attributes: String = properties
                .attributes()
                .filter(|attribute| !matches!(attribute.name(), "b" | "i" | "u" | "sz"))
                .fold(String::new(), |mut text, attribute| {
                    let _ = write!(
                        text,
                        r#" {}="{}""#,
                        attribute.name(),
                        escape(attribute.value())
                    );
                    text
                });
            let start = properties.range().start;
            let kept: String = properties
                .children()
                .filter(Xml::is_element)
                .filter(|child| !matches!(child.tag_name().name(), "solidFill" | "latin"))
                .map(|child| raw[child.range().start - start..child.range().end - start].to_owned())
                .collect();
            new_run_props(&wanted, "a:rPr", &format!("{attributes}\u{0}{kept}"))
        })
    }

    fn paragraph(&self, paragraph: &Node) -> Result<String, String> {
        let attrs = paragraph.attrs();
        let mut xml = String::from("<a:p>");
        xml.push_str(&self.paragraph_props(paragraph)?);
        for child in paragraph.content().children() {
            if let Some(text) = child.text_of() {
                let _ = write!(
                    xml,
                    "<a:r>{}<a:t>{}</a:t></a:r>",
                    self.run_props(child.marks())?,
                    escape(&text)
                );
            } else if schema().node_name(child.kind()) == "hardBreak" {
                xml.push_str("<a:br/>");
            } else {
                let name = child
                    .attrs()
                    .get("raw")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                xml.push_str(self.raw(name)?);
            }
        }
        if let Some(end) = attrs.get("end").and_then(Value::as_str) {
            xml.push_str(self.raw(end)?);
        }
        xml.push_str("</a:p>");
        Ok(xml)
    }

    fn paragraph_props(&self, paragraph: &Node) -> Result<String, String> {
        let attrs = paragraph.attrs();
        let align = attrs.get("align").and_then(Value::as_str);
        let level = attrs.get("level").and_then(Value::as_u64).unwrap_or(0);
        let Some(name) = attrs.get("props").and_then(Value::as_str) else {
            if align.is_none() && level == 0 {
                return Ok(String::new());
            }
            let mut xml = String::from("<a:pPr");
            if level > 0 {
                let _ = write!(xml, r#" lvl="{level}""#);
            }
            if let Some(align) = align {
                let _ = write!(xml, r#" algn="{}""#, escape(align));
            }
            xml.push_str("/>");
            return Ok(xml);
        };
        let raw = self.raw(name)?.to_owned();
        with_piece(&raw, &self.namespaces, |properties| {
            let same = properties.attribute("algn") == align
                && properties
                    .attribute("lvl")
                    .and_then(|level| level.parse::<u64>().ok())
                    .unwrap_or(0)
                    == level;
            if same {
                return raw.clone();
            }
            let mut xml = String::from("<a:pPr");
            for attribute in properties
                .attributes()
                .filter(|attribute| !matches!(attribute.name(), "algn" | "lvl"))
            {
                let _ = write!(
                    xml,
                    r#" {}="{}""#,
                    attribute.name(),
                    escape(attribute.value())
                );
            }
            if level > 0 {
                let _ = write!(xml, r#" lvl="{level}""#);
            }
            if let Some(align) = align {
                let _ = write!(xml, r#" algn="{}""#, escape(align));
            }
            let start = properties.range().start;
            let inner: String = properties
                .children()
                .filter(Xml::is_element)
                .map(|child| raw[child.range().start - start..child.range().end - start].to_owned())
                .collect();
            let _ = write!(xml, ">{inner}</a:pPr>");
            xml
        })
    }

    fn paragraphs(&self, tree: &Node) -> Result<String, String> {
        tree.content()
            .children()
            .iter()
            .map(|paragraph| self.paragraph(paragraph))
            .collect()
    }
}

/// Run properties from a look. `kept` holds other attributes and children, split by a zero.
fn new_run_props(look: &RunLook, tag: &str, kept: &str) -> String {
    let (attributes, children) = kept.split_once('\u{0}').unwrap_or((kept, ""));
    let mut xml = format!("<{tag}{attributes}");
    if look.bold {
        xml.push_str(r#" b="1""#);
    }
    if look.italic {
        xml.push_str(r#" i="1""#);
    }
    if look.underline {
        xml.push_str(r#" u="sng""#);
    }
    if let Some(size) = &look.size {
        let _ = write!(xml, r#" sz="{}""#, escape(size));
    }
    let mut inner = String::new();
    if let Some(color) = &look.color {
        inner.push_str(&fill_xml(color));
    }
    if let Some(font) = &look.font {
        let _ = write!(inner, r#"<a:latin typeface="{}"/>"#, escape(font));
    }
    inner.push_str(children);
    if inner.is_empty() {
        if attributes.is_empty() && xml.len() == tag.len() + 1 {
            return String::new();
        }
        xml.push_str("/>");
    } else {
        let _ = write!(xml, ">{inner}</{tag}>");
    }
    xml
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

fn shape_id(element: Xml<'_, '_>) -> Option<String> {
    let names = element
        .children()
        .filter(Xml::is_element)
        .find(|child| child.tag_name().name().starts_with("nv"))?;
    let properties = names
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "cNvPr")?;
    properties.attribute("id").map(str::to_owned)
}

fn frame_xml(shape: &Shape) -> String {
    let rotation = if shape.rotation == 0 {
        String::new()
    } else {
        format!(r#" rot="{}""#, shape.rotation)
    };
    format!(
        r#"<a:xfrm{rotation}><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        shape.x, shape.y, shape.width, shape.height
    )
}

fn new_shape_xml(out: &Out<'_>, shape: &Shape) -> Result<String, String> {
    let body = match &shape.text {
        Some(tree) => format!(
            "<p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody>",
            out.paragraphs(tree)?
        ),
        None => String::new(),
    };
    let fill = shape.fill.as_deref().map(fill_xml).unwrap_or_default();
    Ok(format!(
        concat!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>"#,
            r#"<p:spPr>{frame}<a:prstGeom prst="{geometry}"><a:avLst/></a:prstGeom>{fill}</p:spPr>{body}</p:sp>"#,
        ),
        id = escape(&shape.id),
        name = escape(&shape.name),
        frame = frame_xml(shape),
        geometry = escape(shape.geometry.as_deref().unwrap_or("rect")),
        fill = fill,
        body = body,
    ))
}

/// Changes one shape's element: only its frame, fill and text, each one when it differs.
fn patch_shape(
    out: &Out<'_>,
    xml: &str,
    element: Xml<'_, '_>,
    old: &Shape,
    new: &Shape,
    edits: &mut Vec<(std::ops::Range<usize>, String)>,
) -> Result<(), String> {
    let same_frame = (old.x, old.y, old.width, old.height, old.rotation)
        == (new.x, new.y, new.width, new.height, new.rotation);
    let properties = local(element, "spPr");
    if !same_frame && let Some(properties) = properties {
        if let Some(transform) = local(properties, "xfrm") {
            edits.push((transform.range(), frame_xml(new)));
        } else if let Some(first) = properties.first_element_child() {
            let at = first.range().start;
            edits.push((at..at, frame_xml(new)));
        } else {
            let range = properties.range();
            let text = &xml[range.clone()];
            let tag = text.trim_end_matches("/>");
            edits.push((range, format!("{tag}>{}</p:spPr>", frame_xml(new))));
        }
    }
    if old.fill != new.fill
        && let Some(properties) = properties
        && let Some(color) = &new.fill
    {
        let fill = properties
            .children()
            .find(|child| child.is_element() && FILLS.contains(&child.tag_name().name()));
        if let Some(fill) = fill {
            edits.push((fill.range(), fill_xml(color)));
        } else if let Some(geometry) = local(properties, "prstGeom")
            .or_else(|| local(properties, "custGeom"))
            .or_else(|| local(properties, "xfrm"))
        {
            let at = geometry.range().end;
            edits.push((at..at, fill_xml(color)));
        }
    }
    if old.text != new.text
        && let (Some(body), Some(tree)) = (local(element, "txBody"), &new.text)
    {
        let paragraphs: Vec<Xml<'_, '_>> = body
            .children()
            .filter(|child| child.is_element() && child.tag_name().name() == "p")
            .collect();
        let replaced = out.paragraphs(tree)?;
        if let (Some(first), Some(last)) = (paragraphs.first(), paragraphs.last()) {
            edits.push((first.range().start..last.range().end, replaced));
        } else {
            let closing = body.range().start + xml[body.range()].rfind("</").unwrap_or(0);
            edits.push((closing..closing, replaced));
        }
    }
    Ok(())
}

fn patch_slide(
    out: &mut Out<'_>,
    xml: &str,
    slide: &Slide,
    on_disk: &Slide,
) -> Result<Option<String>, String> {
    let parsed = Document::parse(xml).map_err(|error| error.to_string())?;
    out.namespaces = namespaces_of(parsed.root_element());
    let tree = parsed
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "spTree")
        .ok_or("the slide has no shapes")?;
    let mut edits = Vec::new();
    let elements: HashMap<String, Xml<'_, '_>> = tree
        .children()
        .filter(Xml::is_element)
        .filter_map(|element| Some((shape_id(element)?, element)))
        .collect();
    for old in &on_disk.shapes {
        let Some(element) = elements.get(&old.id) else {
            continue;
        };
        match slide.shapes.iter().find(|shape| shape.id == old.id) {
            Some(new) if new == old => {}
            Some(new) => patch_shape(out, xml, *element, old, new, &mut edits)?,
            None => edits.push((element.range(), String::new())),
        }
    }
    let end = tree.range().start
        + xml[tree.range()]
            .rfind("</")
            .ok_or("the shapes are broken")?;
    for shape in &slide.shapes {
        if !elements.contains_key(&shape.id) {
            edits.push((end..end, new_shape_xml(out, shape)?));
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
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

fn new_slide_xml(out: &Out<'_>, slide: &Slide) -> Result<String, String> {
    let mut shapes = String::new();
    for shape in &slide.shapes {
        shapes.push_str(&new_shape_xml(out, shape)?);
    }
    Ok(format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="{drawing}" xmlns:r="{relationship}" xmlns:p="{main}">"#,
            r#"<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld>"#,
            r#"<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        ),
        drawing = DRAWING,
        relationship = RELATIONSHIPS,
        main = PRESENTATION_MAIN,
        shapes = shapes,
    ))
}

/// Writes the list of slides, and the parts of slides that are new or gone.
#[expect(clippy::too_many_lines, reason = "one walk through the list of slides")]
fn slide_list(
    model: &Model,
    out: &Out<'_>,
    on_disk: &Model,
    package: &mut Package<'_>,
    parts: &mut Parts,
) -> Result<(), String> {
    let order =
        |slides: &[Slide]| -> Vec<String> { slides.iter().map(|slide| slide.id.clone()).collect() };
    if order(&model.slides) == order(&on_disk.slides) {
        return Ok(());
    }
    let xml = package
        .text(PRESENTATION)
        .ok_or("the presentation is missing")?;
    let parsed = Document::parse(&xml).map_err(|error| error.to_string())?;
    let list = local(parsed.root_element(), "sldIdLst").ok_or("the slide list is missing")?;
    let entries: HashMap<&str, Xml<'_, '_>> = list
        .children()
        .filter(Xml::is_element)
        .filter_map(|entry| Some((entry.attribute("id")?, entry)))
        .collect();
    let relations_xml = package
        .text(PRESENTATION_RELATIONS)
        .ok_or("the relationships are missing")?;
    let types_xml = package
        .text(CONTENT_TYPES)
        .ok_or("the content types are missing")?;
    let layout = on_disk
        .slides
        .first()
        .and_then(|slide| slide.part.as_ref())
        .and_then(|part| {
            package
                .relations(part)
                .into_values()
                .find(|relation| relation.kind == LAYOUT_KIND)
        })
        .map(|relation| relation.target);
    let mut list_xml = String::new();
    let mut added_relations = String::new();
    let mut added_types = String::new();
    for slide in &model.slides {
        if let Some(entry) = entries.get(slide.id.as_str()) {
            list_xml.push_str(&xml[entry.range()]);
            continue;
        }
        let number = slide
            .id
            .parse::<u64>()
            .map_err(|_| "a slide id is not a number")?;
        let name = format!("slideN{number}.xml");
        let part = format!("ppt/slides/{name}");
        let relation = format!("rIdSlide{number}");
        parts.insert(part.clone(), Some(new_slide_xml(out, slide)?.into_bytes()));
        if let Some(layout) = &layout {
            let target = format!("../{}", layout.trim_start_matches("ppt/"));
            parts.insert(
                format!("ppt/slides/_rels/{name}.rels"),
                Some(
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{LAYOUT_KIND}" Target="{target}"/></Relationships>"#
                    )
                    .into_bytes(),
                ),
            );
        }
        let _ = write!(
            list_xml,
            r#"<p:sldId xmlns:r="{RELATIONSHIPS}" id="{number}" r:id="{relation}"/>"#
        );
        let _ = write!(
            added_relations,
            r#"<Relationship Id="{relation}" Type="{SLIDE_KIND}" Target="slides/{name}"/>"#
        );
        let _ = write!(
            added_types,
            r#"<Override PartName="/{part}" ContentType="{SLIDE_TYPE}"/>"#
        );
    }
    let mut relations = relations_xml;
    let mut types = types_xml;
    for slide in &on_disk.slides {
        if model.slide(&slide.id).is_none()
            && let Some(part) = &slide.part
        {
            parts.insert(part.clone(), None);
            let target = part.trim_start_matches("ppt/");
            relations = remove_element_with(&relations, &format!(r#"Target="{target}""#));
            types = remove_element_with(&types, &format!(r#"PartName="/{part}""#));
        }
    }
    let closing = relations
        .rfind("</")
        .ok_or("the relationships are broken")?;
    relations.insert_str(closing, &added_relations);
    let closing = types.rfind("</").ok_or("the content types are broken")?;
    types.insert_str(closing, &added_types);
    let list_text = &xml[list.range()];
    let open_end = list.range().start + list_text.find('>').ok_or("the list is broken")? + 1;
    let close_start = list.range().start + list_text.rfind("</").ok_or("the list is broken")?;
    let new_presentation = format!("{}{list_xml}{}", &xml[..open_end], &xml[close_start..]);
    parts.insert(PRESENTATION.to_owned(), Some(new_presentation.into_bytes()));
    parts.insert(
        PRESENTATION_RELATIONS.to_owned(),
        Some(relations.into_bytes()),
    );
    parts.insert(CONTENT_TYPES.to_owned(), Some(types.into_bytes()));
    Ok(())
}

/// Removes the one tag that holds some text.
fn remove_element_with(xml: &str, needle: &str) -> String {
    let Some(found) = xml.find(needle) else {
        return xml.to_owned();
    };
    let start = xml[..found].rfind('<').unwrap_or(found);
    let end = xml[found..].find("/>").map_or(found, |end| found + end + 2);
    format!("{}{}", &xml[..start], &xml[end..])
}

fn write_parts(model: &Model, file: &[u8], assets: &Assets) -> Result<Option<Vec<u8>>, String> {
    let mut read_assets = assets.clone();
    let on_disk = read::read(file, &mut read_assets).map_err(|Unreadable(problem)| problem)?;
    let mut package = Package::open(file).map_err(|error| error.to_string())?;
    let mut parts = Parts::new();
    let mut out = Out {
        raws: &model.raws,
        namespaces: format!(
            r#" xmlns:a="{DRAWING}" xmlns:r="{RELATIONSHIPS}" xmlns:p="{PRESENTATION_MAIN}""#
        ),
    };
    for old in &on_disk.slides {
        let Some(new) = model.slide(&old.id) else {
            continue;
        };
        if new.shapes == old.shapes {
            continue;
        }
        let Some(part) = &old.part else {
            continue;
        };
        let xml = package.text(part).ok_or("a slide is missing")?;
        if let Some(patched) = patch_slide(&mut out, &xml, new, old)? {
            parts.insert(part.clone(), Some(patched.into_bytes()));
        }
    }
    slide_list(model, &out, &on_disk, &mut package, &mut parts)?;
    if parts.is_empty() {
        return Ok(None);
    }
    rewrite(file, &parts)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub(crate) fn write(model: &Model, file: &[u8], assets: &Assets) -> Result<Vec<u8>, Unwritable> {
    let written = write_parts(model, file, assets).map_err(failed)?;
    Ok(written.unwrap_or_else(|| file.to_vec()))
}

/// Reads a frame back, so tests can name what a slide holds.
#[cfg(test)]
pub(crate) fn frame_in(xml: &str, shape: &str) -> Option<read::Frame> {
    let parsed = Document::parse(xml).ok()?;
    let element = parsed.descendants().find(|node| {
        node.is_element()
            && node.tag_name().name() == "sp"
            && shape_id(*node).as_deref() == Some(shape)
    })?;
    read::frame_of(local(element, "spPr"))
}
