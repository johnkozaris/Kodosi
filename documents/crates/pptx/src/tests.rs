use std::io::Write as _;

use document::{Assets, Format, Ordered, Refused};
use package::Package;
use serde_json::{Value, json};

use crate::{Change, Model, Op, Pptx, Query, ShapeFields, write::frame_in};

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
    let layout_relations = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{RELATIONSHIP}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
    );
    let master = format!(
        r#"<?xml version="1.0"?><p:sldMaster xmlns:a="{DRAWING}" xmlns:p="{MAIN}"><p:cSld><p:spTree><p:nvGrpSpPr/><p:grpSpPr/></p:spTree></p:cSld><p:txStyles><p:otherStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="1800"/></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>"#
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
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            layout_relations,
        ),
        ("ppt/slideMasters/slideMaster1.xml", master),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn linked_deck() -> Vec<u8> {
    let slide = slide_xml("First")
        .replace(
            r#"<a:rPr lang="en-US" b="1" dirty="0"/><a:t>First</a:t></a:r>"#,
            r#"<a:rPr lang="en-US" b="1" dirty="0"><a:hlinkClick r:id="rId2"/></a:rPr><a:t>First</a:t></a:r><a:r><a:rPr lang="en-US"><a:hlinkClick r:id="" action="ppaction://hlinkshowjump?jump=nextslide"/></a:rPr><a:t>Next</a:t></a:r>"#,
        )
        .replace(
            r#"<p:cNvPr id="3" name="Box"/>"#,
            r#"<p:cNvPr id="3" name="Box"><a:hlinkClick r:id="rId3" action="ppaction://hlinksldjump"/></p:cNvPr>"#,
        );
    let relations = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{RELATIONSHIP}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{RELATIONSHIP}/hyperlink" Target="https://kodosi.com/docs" TargetMode="External"/><Relationship Id="rId3" Type="{RELATIONSHIP}/slide" Target="slide2.xml"/></Relationships>"#
    );
    with_parts(
        &deck(),
        &[
            ("ppt/slides/slide1.xml", &slide),
            ("ppt/slides/_rels/slide1.xml.rels", &relations),
        ],
    )
}

/// The deck with a text box in the place of the first slide's box, as python-pptx writes one:
/// with no fill, and a bold run in the color that the box had.
fn labelled_deck() -> Vec<u8> {
    let slide = slide_xml("First");
    let start = slide.find(r#"<p:sp><p:nvSpPr><p:cNvPr id="3""#).unwrap();
    let end = start + slide[start..].find("</p:sp>").unwrap() + "</p:sp>".len();
    let label = concat!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="3" name="TextBox 2"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>"#,
        r#"<p:spPr><a:xfrm><a:off x="12700" y="25400"/><a:ext cx="127000" cy="254000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr>"#,
        r#"<p:txBody><a:bodyPr wrap="square" anchor="ctr"/><a:lstStyle/><a:p><a:r><a:rPr sz="1800" b="1"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:rPr>"#,
        r#"<a:t>Kiln &amp; Co.</a:t></a:r></a:p></p:txBody></p:sp>"#,
    );
    let slide = format!("{}{label}{}", &slide[..start], &slide[end..]);
    with_parts(&deck(), &[("ppt/slides/slide1.xml", &slide)])
}

fn with_parts(file: &[u8], parts: &[(&str, &str)]) -> Vec<u8> {
    let mut original = Package::open(file).unwrap();
    let names: Vec<String> = original.names().map(str::to_owned).collect();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in names {
        let bytes = match parts.iter().find(|(part, _)| *part == name) {
            Some((_, xml)) => xml.as_bytes().to_vec(),
            None => original.bytes(&name).unwrap(),
        };
        writer.start_file(name, options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn read(file: &[u8]) -> Model {
    Pptx::read(file, &mut Assets::default()).unwrap()
}

fn apply_ops(model: &mut Model, ops: Vec<Op>) {
    Pptx::apply(model, Change::Ops { ops }, &[], &mut Assets::default()).unwrap();
}

fn saved(model: &Model, file: &[u8]) -> Vec<u8> {
    Pptx::write(model, file, &Assets::default()).unwrap()
}

fn part(file: &[u8], name: &str) -> String {
    Package::open(file).unwrap().text(name).unwrap()
}

fn listing(model: &Model) -> Value {
    Pptx::query(model, Query::Read { slide: None }).0
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
    let snapshot = Pptx::snapshot(&model).0;
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
                    fill: Some(Some("00FF00".to_owned())),
                    ..ShapeFields::default()
                },
            },
        ],
    };
    Pptx::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    let again = read(&saved(&model, &file));
    let list = listing(&again);
    assert_eq!(list["slides"][1]["shapes"][0]["text"], "Second slide");
    assert_eq!(
        Pptx::snapshot(&again).0["slides"][1]["shapes"][1]["fill"],
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
        Pptx::apply(
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
    let result = Pptx::apply(
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
    match Pptx::apply(&mut model, op, &[earlier], &mut Assets::default()) {
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
    let changes = Pptx::compare(&base, &read(&written));
    assert_eq!(changes.len(), 1, "{changes:?}");
}

#[test]
fn a_write_that_puts_another_shape_under_an_id_brings_that_shape() {
    let file = deck();
    let mut model = read(&file);
    let written = labelled_deck();
    for change in Pptx::compare(&read(&file), &read(&written)) {
        Pptx::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    }
    let label = |model: &Model| {
        model.snapshot()["slides"][0]["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|shape| shape["id"] == "3")
            .cloned()
    };
    assert_eq!(label(&model), label(&read(&written)));
    assert_eq!(saved(&model, &written), written);
}

#[test]
fn a_new_text_box_reads_back_as_it_is_and_keeps_what_people_did_with_it() {
    let file = deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::AddTextBox {
            slide: "256".to_owned(),
            text: "Note".to_owned(),
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 50.0,
        }],
    );
    let with_box = saved(&model, &file);
    let changes = Pptx::compare(&model, &read(&with_box));
    assert!(changes.is_empty(), "{changes:?}");
    let base = model.clone();
    apply_ops(
        &mut model,
        vec![Op::SetText {
            slide: "256".to_owned(),
            shape: "4".to_owned(),
            text: "Note by Ana".to_owned(),
        }],
    );
    let mut other = read(&with_box);
    apply_ops(
        &mut other,
        vec![Op::MoveShape {
            slide: "257".to_owned(),
            shape: "3".to_owned(),
            x: Some(20.0),
            y: None,
            width: None,
            height: None,
        }],
    );
    for change in Pptx::compare(&base, &read(&saved(&other, &with_box))) {
        Pptx::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    }
    assert_eq!(
        listing(&model)["slides"][0]["shapes"][2]["text"],
        "Note by Ana"
    );
}

fn view_text_box(id: &str) -> Value {
    json!({
        "id": id,
        "name": "Text box",
        "kind": "shape",
        "x": 1_000_000,
        "y": 1_000_000,
        "width": 3_000_000,
        "height": 900_000,
        "geometry": "rect",
        "text": {
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Note" }] }],
        },
        "order": 0,
    })
}

fn apply_change(model: &mut Model, change: Change) -> Result<Change, Refused> {
    Pptx::apply(model, change, &[], &mut Assets::default())
}

#[test]
fn a_slide_or_a_shape_takes_only_an_id_that_a_file_can_hold() {
    let file = deck();
    let mut model = read(&file);
    for slide in ["5c1f0d8e-0b6e-4f39-9d3c-1d0e2b7c9a41", "255", "2147483648"] {
        let change = Change::AddSlide {
            slide: slide.to_owned(),
            position: 2.0,
        };
        let refused = apply_change(&mut model, change);
        assert!(matches!(refused, Err(Refused::Invalid(_))), "{slide}");
    }
    let change = Change::AddShape {
        slide: "256".to_owned(),
        shape: view_text_box("9e0a2c44-77b1-4c63-a1f5-0f6d4e2b8c13"),
    };
    assert!(matches!(
        apply_change(&mut model, change),
        Err(Refused::Invalid(_))
    ));
    let slide = "734017113".to_owned();
    let change = Change::AddSlide {
        slide: slide.clone(),
        position: 2.0,
    };
    apply_change(&mut model, change).unwrap();
    let change = Change::AddShape {
        slide: slide.clone(),
        shape: view_text_box("402193877"),
    };
    apply_change(&mut model, change).unwrap();
    let with_slide = saved(&model, &file);
    let changes = Pptx::compare(&model, &read(&with_slide));
    assert!(changes.is_empty(), "{changes:?}");
    apply_ops(
        &mut model,
        vec![Op::SetText {
            slide,
            shape: "402193877".to_owned(),
            text: "Note by Ana".to_owned(),
        }],
    );
    let again = read(&saved(&model, &with_slide));
    assert_eq!(
        listing(&again)["slides"][2]["shapes"][0]["text"],
        "Note by Ana"
    );
    assert_eq!(listing(&again)["slides"].as_array().unwrap().len(), 3);
}

#[test]
fn a_new_shape_takes_an_id_that_no_shape_of_its_slide_holds_in_a_group_too() {
    let group = concat!(
        r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="4" name="Group 3"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>"#,
        r#"<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="10" cy="10"/><a:chOff x="0" y="0"/><a:chExt cx="10" cy="10"/></a:xfrm></p:grpSpPr>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="5" name="Inner"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/></p:sp></p:grpSp>"#,
    );
    let slide = slide_xml("First").replace("</p:spTree>", &format!("{group}</p:spTree>"));
    let file = with_parts(&deck(), &[("ppt/slides/slide1.xml", &slide)]);
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::AddTextBox {
            slide: "256".to_owned(),
            text: "Note".to_owned(),
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 50.0,
        }],
    );
    let xml = part(&saved(&model, &file), "ppt/slides/slide1.xml");
    assert_eq!(xml.matches(r#"<p:cNvPr id="5""#).count(), 1);
    assert!(xml.contains(r#"<p:cNvPr id="6" name="Text 6"/>"#));
}

#[test]
fn a_new_slide_takes_a_free_id_when_the_highest_is_the_last_that_a_file_can_hold() {
    let presentation = part(&deck(), "ppt/presentation.xml")
        .replace(r#"<p:sldId id="257""#, r#"<p:sldId id="2147483647""#);
    let file = with_parts(&deck(), &[("ppt/presentation.xml", &presentation)]);
    let mut model = read(&file);
    apply_ops(&mut model, vec![Op::AddSlide { after: None }]);
    assert_eq!(listing(&model)["slides"][2]["id"], "257");
}

#[test]
fn an_order_carries_over_when_a_write_stacks_shapes_anew() {
    let file = deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::RemoveShape {
            slide: "256".to_owned(),
            shape: "2".to_owned(),
        }],
    );
    let changes = Pptx::compare(&model, &read(&saved(&model, &file)));
    assert!(changes.is_empty(), "{changes:?}");
    let slide = slide_xml("First");
    let title = slide.find(r#"<p:sp><p:nvSpPr><p:cNvPr id="2""#).unwrap();
    let shape = slide.find(r#"<p:sp><p:nvSpPr><p:cNvPr id="3""#).unwrap();
    let end = slide.find("</p:spTree>").unwrap();
    let restacked = format!(
        "{}{}{}{}",
        &slide[..title],
        &slide[shape..end],
        &slide[title..shape],
        &slide[end..]
    );
    let written = with_parts(&file, &[("ppt/slides/slide1.xml", &restacked)]);
    let mut model = read(&file);
    for change in Pptx::compare(&read(&file), &read(&written)) {
        Pptx::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    }
    let orders = |model: &Model| -> Vec<Value> {
        model.snapshot()["slides"][0]["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|shape| json!([shape["id"], shape["order"]]))
            .collect()
    };
    assert_eq!(orders(&model), [json!(["2", 1]), json!(["3", 0])]);
}

#[test]
fn a_new_fill_takes_the_place_of_the_fill_that_the_shape_had() {
    let file = labelled_deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::SetFill {
            slide: "256".to_owned(),
            shape: "3".to_owned(),
            color: "00FF00".to_owned(),
        }],
    );
    let xml = part(&saved(&model, &file), "ppt/slides/slide1.xml");
    assert!(
        xml.contains(
            r#"</a:prstGeom><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></p:spPr>"#
        )
    );
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
    let answer = Pptx::query(&model, Query::Check).0;
    assert_eq!(answer["problems"][0]["shape"], "3");
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = Pptx::guide();
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

#[test]
fn a_link_on_a_run_or_a_shape_says_where_a_click_goes_and_a_save_keeps_it() {
    let file = linked_deck();
    let model = read(&file);
    let snapshot = model.snapshot();
    let shapes = &snapshot["slides"][0]["shapes"];
    let runs = &shapes[0]["text"]["content"][0]["content"];
    let link_of = |run: &Value| {
        run["marks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|mark| mark["type"] == "link")
            .map(|mark| mark["attrs"].clone())
    };
    assert_eq!(
        link_of(&runs[0]),
        Some(json!({ "href": "https://kodosi.com/docs", "slide": null, "jump": null }))
    );
    assert_eq!(
        link_of(&runs[1]),
        Some(json!({ "href": null, "slide": null, "jump": "nextslide" }))
    );
    assert_eq!(shapes[1]["link"], json!({ "slide": "257" }));
    assert_eq!(saved(&model, &file), file);
}
