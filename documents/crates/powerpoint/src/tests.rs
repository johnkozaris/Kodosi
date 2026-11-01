use std::io::Write as _;

use document::{Assets, Format, Ordered, Refused};
use package::Package;
use serde_json::{Value, json};

use crate::{Change, Model, Op, PowerPoint, Query, ShapeFields, write::frame_in};

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const MAIN: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn slide_xml(title: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="{a}" xmlns:r="{r}" xmlns:p="{p}"><p:cSld><p:spTree>"#,
            r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>"#,
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/>"#,
            r#"<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" b="1" dirty="0"/><a:t>{title}</a:t></a:r><a:endParaRPr lang="en-US"/></a:p></p:txBody></p:sp>"#,
            r#"<p:sp><p:nvSpPr><p:cNvPr id="3" name="Box"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr bwMode="auto"><a:xfrm><a:off x="12700" y="25400"/><a:ext cx="127000" cy="254000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></p:spPr></p:sp>"#,
            r#"</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        ),
        a = DRAWING,
        r = RELATIONSHIP,
        p = MAIN,
        title = title,
    )
}

fn deck() -> Vec<u8> {
    let presentation = format!(
        r#"<?xml version="1.0"?><p:presentation xmlns:r="{RELATIONSHIP}" xmlns:p="{MAIN}"><p:sldIdLst><p:sldId id="256" r:id="rId1"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>"#
    );
    let relations = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{RELATIONSHIP}/slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="{RELATIONSHIP}/slide" Target="slides/slide2.xml"/></Relationships>"#
    );
    let layout = format!(
        r#"<?xml version="1.0"?><p:sldLayout xmlns:a="{DRAWING}" xmlns:p="{MAIN}"><p:cSld><p:spTree><p:nvGrpSpPr/><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="100" y="200"/><a:ext cx="300" cy="400"/></a:xfrm></p:spPr></p:sp></p:spTree></p:cSld></p:sldLayout>"#
    );
    let slide_relations = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{RELATIONSHIP}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#
    );
    let types = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/ppt/slides/slide1.xml" ContentType="x"/><Override PartName="/ppt/slides/slide2.xml" ContentType="x"/></Types>"#;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in [
        ("[Content_Types].xml", types.to_owned()),
        ("ppt/presentation.xml", presentation),
        ("ppt/_rels/presentation.xml.rels", relations),
        ("ppt/slides/slide1.xml", slide_xml("First")),
        ("ppt/slides/slide2.xml", slide_xml("Second")),
        ("ppt/slides/_rels/slide1.xml.rels", slide_relations.clone()),
        ("ppt/slides/_rels/slide2.xml.rels", slide_relations),
        ("ppt/slideLayouts/slideLayout1.xml", layout),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn read(file: &[u8]) -> Model {
    PowerPoint::read(file, &mut Assets::default()).unwrap()
}

fn apply_ops(model: &mut Model, ops: Vec<Op>) {
    PowerPoint::apply(model, Change::Ops { ops }, &[], &mut Assets::default()).unwrap();
}

fn saved(model: &Model, file: &[u8]) -> Vec<u8> {
    PowerPoint::write(model, file, &Assets::default()).unwrap()
}

fn part(file: &[u8], name: &str) -> String {
    Package::open(file).unwrap().text(name).unwrap()
}

fn listing(model: &Model) -> Value {
    PowerPoint::query(model, Query::Read { slide: None }).0
}

#[test]
fn no_edit_means_the_same_bytes() {
    let file = deck();
    assert_eq!(saved(&read(&file), &file), file);
}

#[test]
fn slides_and_shapes_come_with_stable_ids_and_inherited_frames() {
    let model = read(&deck());
    let list = listing(&model);
    assert_eq!(list["slides"][0]["id"], "256");
    assert_eq!(list["slides"][1]["shapes"][0]["text"], "Second");
    let snapshot = PowerPoint::snapshot(&model).0;
    let title = &snapshot["slides"][0]["shapes"][0];
    assert_eq!(title["x"], 100);
    assert_eq!(title["height"], 400);
    assert_eq!(snapshot["slides"][0]["shapes"][1]["fill"], "FF0000");
}

#[test]
fn a_move_changes_only_the_frame_of_that_shape() {
    let file = deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::MoveShape {
            slide: "256".to_owned(),
            shape: "3".to_owned(),
            x: Some(10.0),
            y: None,
            width: None,
            height: None,
        }],
    );
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "ppt/slides/slide1.xml");
    assert_eq!(frame_in(&xml, "3").map(|frame| frame.x), Some(127_000));
    assert_eq!(frame_in(&xml, "3").map(|frame| frame.y), Some(25_400));
    assert!(xml.contains(r#"<p:spPr bwMode="auto">"#));
    assert_eq!(
        part(&new_file, "ppt/slides/slide2.xml"),
        part(&file, "ppt/slides/slide2.xml")
    );
}

#[test]
fn new_text_keeps_the_look_of_the_run() {
    let file = deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::ReplaceText {
            find: "First".to_owned(),
            with: "Opening".to_owned(),
            slide: None,
        }],
    );
    let xml = part(&saved(&model, &file), "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"<a:r><a:rPr lang="en-US" b="1" dirty="0"/><a:t>Opening</a:t></a:r>"#));
    assert!(xml.contains(r#"<a:endParaRPr lang="en-US"/>"#));
}

#[test]
fn a_view_sends_steps_for_text_and_fields_for_a_shape() {
    let file = deck();
    let mut model = read(&file);
    let change = Change::Group {
        changes: vec![
            Change::Text {
                slide: "257".to_owned(),
                shape: "2".to_owned(),
                steps: vec![json!({
                    "stepType": "replace",
                    "from": 7,
                    "to": 7,
                    "slice": { "content": [{ "type": "text", "text": " slide" }] },
                })],
            },
            Change::Shape {
                slide: "257".to_owned(),
                shape: "3".to_owned(),
                fields: ShapeFields {
                    fill: Some("00FF00".to_owned()),
                    ..ShapeFields::default()
                },
            },
        ],
    };
    PowerPoint::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    let again = read(&saved(&model, &file));
    let list = listing(&again);
    assert_eq!(list["slides"][1]["shapes"][0]["text"], "Second slide");
    assert_eq!(
        PowerPoint::snapshot(&again).0["slides"][1]["shapes"][1]["fill"],
        "00FF00"
    );
}

#[test]
fn old_text_steps_are_behind_and_a_field_is_not() {
    let mut model = read(&deck());
    let earlier = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: Change::Text {
            slide: "256".to_owned(),
            shape: "2".to_owned(),
            steps: Vec::new(),
        },
    };
    let text = Change::Text {
        slide: "256".to_owned(),
        shape: "2".to_owned(),
        steps: Vec::new(),
    };
    assert!(matches!(
        PowerPoint::apply(
            &mut model,
            text,
            std::slice::from_ref(&earlier),
            &mut Assets::default()
        ),
        Err(Refused::Behind(_))
    ));
    let other_shape = Change::Text {
        slide: "256".to_owned(),
        shape: "3".to_owned(),
        steps: Vec::new(),
    };
    let result = PowerPoint::apply(
        &mut model,
        other_shape,
        std::slice::from_ref(&earlier),
        &mut Assets::default(),
    );
    assert!(matches!(result, Err(Refused::Invalid(_))));
    let op = Change::Ops {
        ops: vec![Op::SetText {
            slide: "256".to_owned(),
            shape: "2".to_owned(),
            text: "late".to_owned(),
        }],
    };
    match PowerPoint::apply(&mut model, op, &[earlier], &mut Assets::default()) {
        Err(Refused::Behind(detail)) => assert!(detail.contains("Ana")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn slides_can_be_added_moved_and_removed() {
    let file = deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![
            Op::AddSlide {
                after: Some("256".to_owned()),
            },
            Op::MoveSlide {
                slide: "257".to_owned(),
                after: None,
            },
        ],
    );
    apply_ops(
        &mut model,
        vec![Op::AddTextBox {
            slide: "258".to_owned(),
            text: "New words".to_owned(),
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 50.0,
        }],
    );
    let new_file = saved(&model, &file);
    let again = read(&new_file);
    let list = listing(&again);
    let order: Vec<&str> = list["slides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slide| slide["id"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["257", "256", "258"]);
    assert_eq!(list["slides"][2]["shapes"][0]["text"], "New words");

    let mut smaller = again;
    apply_ops(
        &mut smaller,
        vec![Op::RemoveSlide {
            slide: "256".to_owned(),
        }],
    );
    let final_file = saved(&smaller, &new_file);
    assert!(
        !Package::open(&final_file)
            .unwrap()
            .has("ppt/slides/slide1.xml")
    );
    assert_eq!(
        listing(&read(&final_file))["slides"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_compare_finds_only_what_a_write_changed() {
    let file = deck();
    let base = read(&file);
    let mut other = read(&file);
    apply_ops(
        &mut other,
        vec![Op::SetFill {
            slide: "257".to_owned(),
            shape: "3".to_owned(),
            color: "0000FF".to_owned(),
        }],
    );
    let written = saved(&other, &file);
    let changes = PowerPoint::compare(&base, &read(&written));
    assert_eq!(changes.len(), 1, "{changes:?}");
}

#[test]
fn check_finds_a_shape_that_leaves_the_slide() {
    let mut model = read(&deck());
    apply_ops(
        &mut model,
        vec![Op::MoveShape {
            slide: "256".to_owned(),
            shape: "3".to_owned(),
            x: Some(100_000.0),
            y: None,
            width: None,
            height: None,
        }],
    );
    let answer = PowerPoint::query(&model, Query::Check).0;
    assert_eq!(answer["problems"][0]["shape"], "3");
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = PowerPoint::guide();
    for op in &guide.ops {
        let mut example = json!({ "op": op.name });
        for field in op.fields.iter().filter(|field| field.required) {
            example[&field.name] = match field.kind.as_str() {
                "points" => json!(1.0),
                _ => json!("x"),
            };
        }
        let parsed: Result<Op, _> = serde_json::from_value(example);
        assert!(parsed.is_ok(), "{}: {parsed:?}", op.name);
    }
}
