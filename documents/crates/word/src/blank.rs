use std::{fmt::Write as _, io::Write as _};

use package::{escape, spans};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="xml" ContentType="application/xml"/>"#,
    r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    r#"<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#,
    r#"<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#,
    r#"</Types>"#,
);
const ROOT_RELATIONS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>"#,
    r#"</Relationships>"#,
);
const RELATIONS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
    r#"</Relationships>"#,
);

fn heading_style(level: u32, size: u32) -> String {
    format!(
        r#"<w:style w:type="paragraph" w:styleId="Heading{level}"><w:name w:val="heading {level}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/></w:rPr></w:style>"#,
        level - 1
    )
}

fn styles() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{main}">"#,
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>"#,
            r#"<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>"#,
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>"#,
            r#"{one}{two}{three}"#,
            r#"<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:left="720"/></w:pPr></w:style>"#,
            r#"</w:styles>"#,
        ),
        main = MAIN,
        one = heading_style(1, 40),
        two = heading_style(2, 32),
        three = heading_style(3, 26),
    )
}

const BULLET_LIST: u32 = 1;
const NUMBER_LIST: u32 = 2;
const BULLETS: [&str; 3] = ["•", "◦", "▪"];
const NUMBERS: [&str; 3] = ["decimal", "lowerLetter", "lowerRoman"];

/// A list with nine levels, each one step further in.
fn list(id: u32, level: impl Fn(usize) -> (&'static str, String)) -> String {
    let mut xml = format!(
        r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="hybridMultilevel"/>"#
    );
    for depth in 0..9 {
        let (format, text) = level(depth);
        let _ = write!(
            xml,
            r#"<w:lvl w:ilvl="{depth}"><w:start w:val="1"/><w:numFmt w:val="{format}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{}" w:hanging="360"/></w:pPr></w:lvl>"#,
            720 * (depth + 1)
        );
    }
    xml.push_str("</w:abstractNum>");
    xml
}

fn numbering() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{main}">"#,
            r#"{bullets}{numbers}"#,
            r#"<w:num w:numId="{bullet_list}"><w:abstractNumId w:val="0"/></w:num>"#,
            r#"<w:num w:numId="{number_list}"><w:abstractNumId w:val="1"/></w:num>"#,
            r#"</w:numbering>"#,
        ),
        main = MAIN,
        bullet_list = BULLET_LIST,
        number_list = NUMBER_LIST,
        bullets = list(0, |depth| ("bullet", BULLETS[depth % 3].to_owned())),
        numbers = list(1, |depth| (NUMBERS[depth % 3], format!("%{}.", depth + 1))),
    )
}

fn paragraph(properties: &str, text: &str) -> String {
    let mut runs = String::new();
    for span in spans(text) {
        let look = match (span.bold, span.italic) {
            (false, false) => "",
            (true, false) => "<w:rPr><w:b/></w:rPr>",
            (false, true) => "<w:rPr><w:i/></w:rPr>",
            (true, true) => "<w:rPr><w:b/><w:i/></w:rPr>",
        };
        let _ = write!(
            runs,
            r#"<w:r>{look}<w:t xml:space="preserve">{}</w:t></w:r>"#,
            escape(&span.text)
        );
    }
    let properties = if properties.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{properties}</w:pPr>")
    };
    format!("<w:p>{properties}{runs}</w:p>")
}

/// A Markdown list item: its list, bullets or numbers, and its words.
fn list_item(line: &str) -> Option<(u32, &str)> {
    if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some((BULLET_LIST, item));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let item = line[digits..].strip_prefix(". ")?;
    (digits > 0).then_some((NUMBER_LIST, item))
}

fn body_of(markdown: &str) -> String {
    let mut body = String::new();
    for line in markdown.lines() {
        let line = line.trim_end();
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=3).contains(&hashes) && line[hashes..].starts_with(' ') {
            let style = format!(r#"<w:pStyle w:val="Heading{hashes}"/>"#);
            body.push_str(&paragraph(&style, line[hashes..].trim()));
        } else if let Some((list, item)) = list_item(line) {
            let style = format!(
                r#"<w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="{list}"/></w:numPr>"#
            );
            body.push_str(&paragraph(&style, item));
        } else if !line.is_empty() {
            body.push_str(&paragraph("", line));
        }
    }
    if body.is_empty() {
        body.push_str("<w:p/>");
    }
    body
}

/// A new Word file, from Markdown when there is some.
pub fn new_file(markdown: Option<&str>) -> Result<Vec<u8>, String> {
    let mut document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{MAIN}"><w:body>"#
    );
    document.push_str(&body_of(markdown.unwrap_or_default()));
    let _ = write!(
        document,
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let error = |error: zip::result::ZipError| error.to_string();
    for (name, bytes) in [
        ("[Content_Types].xml", TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELATIONS.to_owned()),
        ("word/document.xml", document),
        ("word/styles.xml", styles()),
        ("word/numbering.xml", numbering()),
        ("word/_rels/document.xml.rels", RELATIONS.to_owned()),
    ] {
        writer.start_file(name, options).map_err(error)?;
        writer
            .write_all(bytes.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Ok(writer.finish().map_err(error)?.into_inner())
}
