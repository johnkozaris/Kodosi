use std::{fmt::Write as _, io::Write as _};

use package::{escape, spans};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const MAIN: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_RELATIONS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
const WIDTH: i64 = 12_192_000;
const HEIGHT: i64 = 6_858_000;
const SLIDE_TYPE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";

fn relations(items: &[(&str, &str, &str)]) -> String {
    let mut xml = format!(r#"{HEADER}<Relationships xmlns="{PACKAGE_RELATIONS}">"#);
    for (id, kind, target) in items {
        let _ = write!(
            xml,
            r#"<Relationship Id="{id}" Type="{RELATIONSHIP}/{kind}" Target="{target}"/>"#
        );
    }
    xml.push_str("</Relationships>");
    xml
}

fn tree(shapes: &str) -> String {
    format!(
        r#"<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld>"#
    )
}

fn placeholder(id: u32, name: &str, kind: &str, frame: [i64; 4], body: &str) -> String {
    let [x, y, width, height] = frame;
    format!(
        concat!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr><p:ph type="{kind}"/></p:nvPr></p:nvSpPr>"#,
            r#"<p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm></p:spPr>"#,
            r#"<p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp>"#,
        ),
        id = id,
        name = name,
        kind = kind,
        x = x,
        y = y,
        width = width,
        height = height,
        body = body,
    )
}

/// The look of text on the master: a heading, a body with bullets, and all other text.
fn text_styles() -> String {
    let level = |number: u32, margin: u32, size: u32| {
        format!(
            concat!(
                r#"<a:lvl{number}pPr marL="{margin}" indent="-228600"><a:buChar char="•"/><a:defRPr sz="{size}">"#,
                r#"<a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/></a:defRPr></a:lvl{number}pPr>"#,
            ),
            number = number,
            margin = margin,
            size = size,
        )
    };
    format!(
        concat!(
            r#"<p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="4400"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill>"#,
            r#"<a:latin typeface="+mj-lt"/></a:defRPr></a:lvl1pPr></p:titleStyle>"#,
            r#"<p:bodyStyle>{one}{two}{three}</p:bodyStyle>"#,
            r#"<p:otherStyle><a:lvl1pPr><a:defRPr sz="1800"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill>"#,
            r#"<a:latin typeface="+mn-lt"/></a:defRPr></a:lvl1pPr></p:otherStyle></p:txStyles>"#,
        ),
        one = level(1, 228_600, 2800),
        two = level(2, 685_800, 2400),
        three = level(3, 1_143_000, 2000),
    )
}

fn theme() -> String {
    format!(
        concat!(
            r#"{header}<a:theme xmlns:a="{a}" name="Plain"><a:themeElements><a:clrScheme name="Plain">"#,
            r#"<a:dk1><a:srgbClr val="1A1A1A"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="333333"/></a:dk2><a:lt2><a:srgbClr val="EEEEEE"/></a:lt2>"#,
            r#"<a:accent1><a:srgbClr val="B5651D"/></a:accent1><a:accent2><a:srgbClr val="4472C4"/></a:accent2><a:accent3><a:srgbClr val="70AD47"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="ED7D31"/></a:accent6>"#,
            r#"<a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme>"#,
            r#"<a:fontScheme name="Plain"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>"#,
            r#"<a:fmtScheme name="Plain"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>"#,
            r#"<a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>"#,
            r#"<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>"#,
            r#"<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>"#,
            r#"</a:themeElements></a:theme>"#,
        ),
        header = HEADER,
        a = DRAWING,
    )
}

const TITLE_FRAME: [i64; 4] = [838_200, 365_125, 10_515_600, 1_325_563];
const BODY_FRAME: [i64; 4] = [838_200, 1_825_625, 10_515_600, 4_351_338];

fn master() -> String {
    let empty = "<a:p><a:endParaRPr/></a:p>";
    let shapes = placeholder(2, "Title", "title", TITLE_FRAME, empty)
        .replace("<a:bodyPr/>", r#"<a:bodyPr anchor="ctr"/>"#)
        + &placeholder(3, "Body", "body", BODY_FRAME, empty);
    format!(
        concat!(
            r#"{header}<p:sldMaster xmlns:a="{a}" xmlns:r="{r}" xmlns:p="{p}">{tree}"#,
            r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#,
            r#"<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>{styles}</p:sldMaster>"#,
        ),
        header = HEADER,
        a = DRAWING,
        r = RELATIONSHIP,
        p = MAIN,
        tree = tree(&shapes),
        styles = text_styles(),
    )
}

fn layout() -> String {
    let empty = "<a:p><a:endParaRPr/></a:p>";
    let shapes = placeholder(2, "Title", "title", TITLE_FRAME, empty)
        + &placeholder(3, "Body", "body", BODY_FRAME, empty);
    format!(
        r#"{HEADER}<p:sldLayout xmlns:a="{DRAWING}" xmlns:r="{RELATIONSHIP}" xmlns:p="{MAIN}" type="obj">{}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        tree(&shapes)
    )
}

fn paragraphs(lines: &[String]) -> String {
    if lines.is_empty() {
        return "<a:p><a:endParaRPr/></a:p>".to_owned();
    }
    lines.iter().fold(String::new(), |mut xml, line| {
        xml.push_str("<a:p>");
        for span in spans(line) {
            let on = |set: bool, name: &str| {
                if set {
                    format!(r#" {name}="1""#)
                } else {
                    String::new()
                }
            };
            let _ = write!(
                xml,
                r#"<a:r><a:rPr lang="en-US"{}{}/><a:t>{}</a:t></a:r>"#,
                on(span.bold, "b"),
                on(span.italic, "i"),
                escape(&span.text)
            );
        }
        xml.push_str("</a:p>");
        xml
    })
}

fn slide(title: &str, lines: &[String]) -> String {
    let shapes = placeholder(
        2,
        "Title",
        "title",
        TITLE_FRAME,
        &paragraphs(&[title.to_owned()]),
    ) + &placeholder(3, "Body", "body", BODY_FRAME, &paragraphs(lines));
    format!(
        r#"{HEADER}<p:sld xmlns:a="{DRAWING}" xmlns:r="{RELATIONSHIP}" xmlns:p="{MAIN}">{}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        tree(&shapes)
    )
}

/// Slides from Markdown: each heading starts a slide, and the lines below it are its body.
fn slides_of(markdown: &str) -> Vec<(String, Vec<String>)> {
    let mut slides: Vec<(String, Vec<String>)> = Vec::new();
    for line in markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(title) = line.strip_prefix('#') {
            slides.push((title.trim_start_matches('#').trim().to_owned(), Vec::new()));
        } else {
            if slides.is_empty() {
                slides.push((String::new(), Vec::new()));
            }
            let body = line.trim_start_matches(['-', '*', ' ']);
            if let Some((_, lines)) = slides.last_mut() {
                lines.push(body.to_owned());
            }
        }
    }
    if slides.is_empty() {
        slides.push((String::new(), Vec::new()));
    }
    slides
}

/// A new `PowerPoint` file, from Markdown when there is some.
#[expect(clippy::too_many_lines, reason = "the list of parts of a deck")]
pub fn new_file(markdown: Option<&str>) -> Result<Vec<u8>, String> {
    let slides = slides_of(markdown.unwrap_or_default());
    let mut list = String::new();
    let mut presentation_relations = vec![
        (
            "rId1".to_owned(),
            "slideMaster",
            "slideMasters/slideMaster1.xml".to_owned(),
        ),
        ("rId2".to_owned(), "theme", "theme/theme1.xml".to_owned()),
    ];
    let mut types = format!(
        concat!(
            r#"{header}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
            r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
            r#"<Default Extension="xml" ContentType="application/xml"/>"#,
            r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>"#,
            r#"<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>"#,
            r#"<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#,
            r#"<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#,
        ),
        header = HEADER,
    );
    for number in 1..=slides.len() {
        let _ = write!(
            list,
            r#"<p:sldId id="{}" r:id="rId{}"/>"#,
            255 + number,
            number + 2
        );
        presentation_relations.push((
            format!("rId{}", number + 2),
            "slide",
            format!("slides/slide{number}.xml"),
        ));
        let _ = write!(
            types,
            r#"<Override PartName="/ppt/slides/slide{number}.xml" ContentType="{SLIDE_TYPE}"/>"#
        );
    }
    types.push_str("</Types>");
    let presentation = format!(
        concat!(
            r#"{header}<p:presentation xmlns:a="{a}" xmlns:r="{r}" xmlns:p="{p}">"#,
            r#"<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>"#,
            r#"<p:sldIdLst>{list}</p:sldIdLst><p:sldSz cx="{width}" cy="{height}"/><p:notesSz cx="{height}" cy="{width}"/></p:presentation>"#,
        ),
        header = HEADER,
        a = DRAWING,
        r = RELATIONSHIP,
        p = MAIN,
        list = list,
        width = WIDTH,
        height = HEIGHT,
    );
    let borrowed: Vec<(&str, &str, &str)> = presentation_relations
        .iter()
        .map(|(id, kind, target)| (id.as_str(), *kind, target.as_str()))
        .collect();
    let mut parts: Vec<(String, String)> = vec![
        ("[Content_Types].xml".to_owned(), types),
        (
            "_rels/.rels".to_owned(),
            relations(&[("rId1", "officeDocument", "ppt/presentation.xml")]),
        ),
        ("ppt/presentation.xml".to_owned(), presentation),
        (
            "ppt/_rels/presentation.xml.rels".to_owned(),
            relations(&borrowed),
        ),
        ("ppt/theme/theme1.xml".to_owned(), theme()),
        ("ppt/slideMasters/slideMaster1.xml".to_owned(), master()),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_owned(),
            relations(&[
                ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                ("rId2", "theme", "../theme/theme1.xml"),
            ]),
        ),
        ("ppt/slideLayouts/slideLayout1.xml".to_owned(), layout()),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_owned(),
            relations(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]),
        ),
    ];
    for (number, (title, lines)) in (1..).zip(&slides) {
        parts.push((format!("ppt/slides/slide{number}.xml"), slide(title, lines)));
        parts.push((
            format!("ppt/slides/_rels/slide{number}.xml.rels"),
            relations(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")]),
        ));
    }
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let error = |error: zip::result::ZipError| error.to_string();
    for (name, bytes) in parts {
        writer.start_file(name, options).map_err(error)?;
        writer
            .write_all(bytes.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Ok(writer.finish().map_err(error)?.into_inner())
}
