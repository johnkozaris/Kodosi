use std::{collections::BTreeMap, io::Write as _};

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

/// Replaces the first slide's box with a python-pptx-style text box (`<a:noFill/>` and a bold
/// run in the box's former color).
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
            keep: BTreeMap::new(),
        },
    };
    let text = Change::Text {
        slide: "256".to_owned(),
        shape: "2".to_owned(),
        steps: Vec::new(),
        keep: BTreeMap::new(),
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
        keep: BTreeMap::new(),
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
fn a_slide_keeps_its_id_when_the_list_names_its_part_first() {
    let presentation = format!(
        r#"<?xml version="1.0"?><p:presentation xmlns:r="{RELATIONSHIP}" xmlns:p="{MAIN}"><p:sldIdLst><p:sldId r:id="rId1" id="256"/><p:sldId r:id="rId2" id="257"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>"#
    );
    let file = with_parts(&deck(), &[("ppt/presentation.xml", &presentation)]);
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![Op::RemoveSlide {
            slide: "256".to_owned(),
        }],
    );
    let list = listing(&read(&saved(&model, &file)));
    assert_eq!(list["slides"].as_array().unwrap().len(), 1);
    assert_eq!(list["slides"][0]["id"], "257");
    assert_eq!(list["slides"][0]["shapes"][0]["text"], "Second");
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
        keep: BTreeMap::new(),
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
        keep: BTreeMap::new(),
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
fn a_shape_that_a_save_took_away_comes_back_whole_and_can_again() {
    let file = deck();
    let mut model = read(&file);
    let title = Pptx::snapshot(&model).0["slides"][0]["shapes"][0].clone();
    apply_ops(
        &mut model,
        vec![Op::RemoveShape {
            slide: "256".to_owned(),
            shape: "2".to_owned(),
        }],
    );
    let without = saved(&model, &file);
    assert!(!part(&without, "ppt/slides/slide1.xml").contains("<p:ph"));
    let back = Change::AddShape {
        slide: "256".to_owned(),
        shape: title,
        keep: BTreeMap::new(),
    };
    Pptx::apply(&mut model, back, &[], &mut Assets::default()).unwrap();
    let with = saved(&model, &without);
    let xml = part(&with, "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"<p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/>"#));
    assert!(xml.contains("<a:t>First</a:t>"));
    let again = Pptx::snapshot(&read(&with)).0;
    let returned = &again["slides"][0]["shapes"][1];
    assert_eq!(returned["placeholder"], "title:0");
    assert!(returned["source"].is_string());
}

#[test]
fn a_change_that_the_file_could_not_save_is_refused_and_changes_nothing() {
    let mut model = read(&deck());
    let before = model.clone();
    let title = |steps: Vec<Value>| Change::Text {
        slide: "256".to_owned(),
        shape: "2".to_owned(),
        steps,
        keep: BTreeMap::new(),
    };
    let typed = |node: Value| {
        title(vec![json!({
            "stepType": "replace",
            "from": 1,
            "to": 1,
            "slice": { "content": [node] },
        })])
    };
    let added = |shape: Value| Change::AddShape {
        slide: "256".to_owned(),
        shape,
        keep: BTreeMap::new(),
    };
    let unknown = "0123456789abcdef";
    let mut picture = view_text_box("9");
    picture["kind"] = json!("picture");
    picture["asset"] = json!("ab".repeat(32));
    let mut placeholder = view_text_box("10");
    placeholder["placeholder"] = json!("title:0");
    let mut stray = view_text_box("11");
    stray["source"] = json!(unknown);
    stray["geometry"] = json!("ellipse");
    let mut styled = view_text_box("12");
    styled["text"]["content"][0]["content"][0]["marks"] =
        json!([{ "type": "run", "attrs": { "props": unknown } }]);
    let mut borrowed = Pptx::snapshot(&model).0["slides"][0]["shapes"][1].clone();
    borrowed["id"] = json!("13");
    let mut shaped = view_text_box("14");
    shaped["geometry"] = json!("bogus");
    let framed = |fields: ShapeFields| Change::Shape {
        slide: "256".to_owned(),
        shape: "3".to_owned(),
        fields,
    };
    for change in [
        typed(json!({ "type": "inlineOpaque", "attrs": { "raw": null } })),
        typed(json!({ "type": "inlineOpaque", "attrs": { "raw": unknown } })),
        typed(json!({
            "type": "text",
            "text": "x",
            "marks": [{ "type": "run", "attrs": { "props": unknown } }],
        })),
        title(vec![
            json!({ "stepType": "attr", "pos": 0, "attr": "props", "value": unknown }),
        ]),
        typed(json!({ "type": "video" })),
        added(picture),
        added(placeholder),
        added(stray),
        added(styled),
        added(borrowed),
        added(shaped),
        framed(ShapeFields {
            width: Some(-1),
            ..ShapeFields::default()
        }),
        framed(ShapeFields {
            rotation: Some(3_000_000_000),
            ..ShapeFields::default()
        }),
        Change::Group {
            changes: vec![
                framed(ShapeFields {
                    x: Some(0),
                    ..ShapeFields::default()
                }),
                typed(json!({ "type": "inlineOpaque", "attrs": { "raw": null } })),
            ],
        },
    ] {
        assert!(
            matches!(
                apply_change(&mut model, change.clone()),
                Err(Refused::Invalid(_))
            ),
            "{change:?}"
        );
        assert_eq!(model, before);
    }
    let off_the_slide = framed(ShapeFields {
        x: Some(-12_700),
        width: Some(0),
        ..ShapeFields::default()
    });
    assert!(apply_change(&mut model, off_the_slide).is_ok());
    let broken = slide_xml("First").replace(r#"cx="127000""#, r#"cx="-127000""#);
    let mut model = read(&with_parts(&deck(), &[("ppt/slides/slide1.xml", &broken)]));
    let moved = framed(ShapeFields {
        x: Some(0),
        ..ShapeFields::default()
    });
    assert!(apply_change(&mut model, moved).is_ok());
}

#[test]
fn a_person_or_an_agent_cannot_send_what_the_file_does_not_take() {
    let pieces = BTreeMap::from([("0123456789abcdef".to_owned(), "<a:rPr/>".to_owned())]);
    let fields = |fields: ShapeFields| Change::Shape {
        slide: "256".to_owned(),
        shape: "3".to_owned(),
        fields,
    };
    let fill = |color: Option<&str>| {
        fields(ShapeFields {
            fill: Some(color.map(str::to_owned)),
            ..ShapeFields::default()
        })
    };
    let added = |shape: Value, keep: BTreeMap<String, String>| Change::AddShape {
        slide: "256".to_owned(),
        shape,
        keep,
    };
    let mut red = view_text_box("9");
    red["fill"] = json!("red");
    for change in [
        Change::Text {
            slide: "256".to_owned(),
            shape: "2".to_owned(),
            steps: Vec::new(),
            keep: pieces.clone(),
        },
        added(view_text_box("9"), pieces),
        added(red, BTreeMap::new()),
        fields(ShapeFields {
            line: Some(Some("FF0000".to_owned())),
            ..ShapeFields::default()
        }),
        fields(ShapeFields {
            order: Some(3),
            ..ShapeFields::default()
        }),
        fill(None),
        fill(Some("red")),
        Change::Group {
            changes: vec![fill(Some("00FF00")), fill(None)],
        },
    ] {
        assert!(Pptx::sendable(&change).is_err(), "{change:?}");
    }
    for change in [
        fields(ShapeFields {
            x: Some(0),
            y: Some(0),
            width: Some(9),
            height: Some(9),
            rotation: Some(0),
            ..ShapeFields::default()
        }),
        fill(Some("00FF00")),
        fill(Some("scheme:accent1")),
        added(view_text_box("9"), BTreeMap::new()),
    ] {
        assert!(Pptx::sendable(&change).is_ok(), "{change:?}");
    }
    let mut model = read(&deck());
    let named = Change::Ops {
        ops: vec![Op::SetFill {
            slide: "256".to_owned(),
            shape: "3".to_owned(),
            color: "red".to_owned(),
        }],
    };
    assert!(matches!(
        apply_change(&mut model, named),
        Err(Refused::Invalid(_))
    ));
}

#[test]
fn a_merge_brings_the_pieces_of_a_write_so_that_later_edits_still_save() {
    let file = deck();
    let mut model = read(&file);
    let italic = slide_xml("First").replace(r#"b="1" dirty="0""#, r#"b="1" i="1" dirty="0""#);
    let written = with_parts(&file, &[("ppt/slides/slide1.xml", &italic)]);
    for change in Pptx::compare(&model, &read(&written)) {
        apply_change(&mut model, change).unwrap();
    }
    let typed = Change::Text {
        slide: "256".to_owned(),
        shape: "2".to_owned(),
        steps: vec![json!({
            "stepType": "replace",
            "from": 1,
            "to": 1,
            "slice": { "content": [{ "type": "text", "text": "The " }] },
        })],
        keep: BTreeMap::new(),
    };
    apply_change(&mut model, typed).unwrap();
    let xml = part(&saved(&model, &written), "ppt/slides/slide1.xml");
    assert!(xml.contains("<a:t>The </a:t>"));
    assert!(xml.contains(r#"<a:rPr lang="en-US" b="1" i="1" dirty="0"/><a:t>First</a:t>"#));
}

fn turned_deck() -> Vec<u8> {
    let slide = slide_xml("First")
        .replace("<a:xfrm><a:off", r#"<a:xfrm flipH="1"><a:off"#)
        .replace(
            r#"<a:srgbClr val="FF0000"/>"#,
            r#"<a:srgbClr val="FF0000"><a:lumMod val="75000"/></a:srgbClr>"#,
        );
    with_parts(&deck(), &[("ppt/slides/slide1.xml", &slide)])
}

fn box_moved_to(x: f64) -> Op {
    Op::MoveShape {
        slide: "256".to_owned(),
        shape: "3".to_owned(),
        x: Some(x),
        y: None,
        width: None,
        height: None,
    }
}

#[test]
fn a_move_keeps_a_shape_turned_over() {
    let file = turned_deck();
    let mut model = read(&file);
    apply_ops(&mut model, vec![box_moved_to(10.0)]);
    let xml = part(&saved(&model, &file), "ppt/slides/slide1.xml");
    assert!(xml.contains(
        r#"<a:xfrm flipH="1"><a:off x="127000" y="25400"/><a:ext cx="127000" cy="254000"/></a:xfrm>"#
    ));
}

#[test]
fn a_shape_that_comes_back_keeps_the_turn_and_the_colour_of_its_piece() {
    let file = turned_deck();
    let mut model = read(&file);
    apply_ops(&mut model, vec![box_moved_to(10.0)]);
    let moved = saved(&model, &file);
    let shape = Pptx::snapshot(&model).0["slides"][0]["shapes"][1].clone();
    apply_ops(
        &mut model,
        vec![Op::RemoveShape {
            slide: "256".to_owned(),
            shape: "3".to_owned(),
        }],
    );
    let without = saved(&model, &moved);
    let back = Change::AddShape {
        slide: "256".to_owned(),
        shape,
        keep: BTreeMap::new(),
    };
    Pptx::apply(&mut model, back, &[], &mut Assets::default()).unwrap();
    let xml = part(&saved(&model, &without), "ppt/slides/slide1.xml");
    assert!(xml.contains(
        r#"<a:xfrm flipH="1"><a:off x="127000" y="25400"/><a:ext cx="127000" cy="254000"/></a:xfrm>"#
    ));
    assert!(xml.contains(
        r#"<a:solidFill><a:srgbClr val="FF0000"><a:lumMod val="75000"/></a:srgbClr></a:solidFill>"#
    ));
}

#[test]
fn a_shape_comes_back_from_its_piece_only_on_the_slide_that_held_it() {
    let mut model = read(&deck());
    let title = Pptx::snapshot(&model).0["slides"][0]["shapes"][0].clone();
    apply_ops(
        &mut model,
        vec![Op::RemoveShape {
            slide: "257".to_owned(),
            shape: "2".to_owned(),
        }],
    );
    let before = model.clone();
    let elsewhere = Change::AddShape {
        slide: "257".to_owned(),
        shape: title,
        keep: BTreeMap::new(),
    };
    assert!(matches!(
        apply_change(&mut model, elsewhere),
        Err(Refused::Invalid(_))
    ));
    assert_eq!(model, before);
}

#[test]
fn a_shape_that_another_program_wrote_can_come_back_on_its_slide() {
    let file = deck();
    let mut model = read(&file);
    let ellipse = r#"<p:sp><p:nvSpPr><p:cNvPr id="4" name="Mark"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="127000" cy="127000"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom></p:spPr></p:sp></p:spTree>"#;
    let marked = slide_xml("First").replace("</p:spTree>", ellipse);
    let written = with_parts(&file, &[("ppt/slides/slide1.xml", &marked)]);
    for change in Pptx::compare(&model, &read(&written)) {
        apply_change(&mut model, change).unwrap();
    }
    let mark = Pptx::snapshot(&model).0["slides"][0]["shapes"][2].clone();
    assert_eq!(mark["geometry"], "ellipse");
    apply_ops(
        &mut model,
        vec![Op::RemoveShape {
            slide: "256".to_owned(),
            shape: "4".to_owned(),
        }],
    );
    let back = Change::AddShape {
        slide: "256".to_owned(),
        shape: mark,
        keep: BTreeMap::new(),
    };
    apply_change(&mut model, back).unwrap();
    let xml = part(&saved(&model, &written), "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"<a:prstGeom prst="ellipse">"#));
}

#[test]
fn a_box_whose_piece_went_comes_back_as_a_new_box_with_its_words() {
    let note = concat!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="5" name="Note"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>"#,
        r#"<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="127000" cy="127000"/></a:xfrm>"#,
        r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>"#,
        r#"<a:p><a:r><a:rPr lang="fr-FR" sz="1400" b="1"/><a:t>Notes </a:t></a:r>"#,
        r#"<a:fld id="{1}" type="slidenum"><a:rPr lang="fr-FR"/><a:t>1</a:t></a:fld>"#,
        r#"<a:endParaRPr lang="fr-FR"/></a:p><a:p><a:r><a:rPr lang="fr-FR"/><a:t>A</a:t></a:r>"#,
        r#"<a:r><a:rPr lang="de-DE"/><a:t>B</a:t></a:r></a:p></p:txBody></p:sp></p:spTree>"#,
    );
    let slide = slide_xml("First").replace("</p:spTree>", note);
    let file = with_parts(&deck(), &[("ppt/slides/slide1.xml", &slide)]);
    let mut model = read(&file);
    let shapes = Pptx::snapshot(&model).0["slides"][0]["shapes"].clone();
    let removed = |shape: &str| Op::RemoveShape {
        slide: "256".to_owned(),
        shape: shape.to_owned(),
    };
    apply_ops(&mut model, vec![removed("2"), removed("5")]);
    let saved_without = saved(&model, &file);
    let mut reopened = read(&saved_without);
    let back = |shape: &Value| Change::AddShape {
        slide: "256".to_owned(),
        shape: shape.clone(),
        keep: BTreeMap::new(),
    };
    assert!(matches!(
        apply_change(&mut reopened, back(&shapes[0])),
        Err(Refused::Invalid(_))
    ));
    let applied = serde_json::to_value(apply_change(&mut reopened, back(&shapes[2])).unwrap());
    let applied = applied.unwrap()["shape"].clone();
    assert!(applied["source"].is_null());
    assert_eq!(
        applied["text"]["content"][1]["content"],
        json!([{ "type": "text", "text": "AB", "marks": [{ "type": "run", "attrs": { "props": null } }] }])
    );
    let xml = part(&saved(&reopened, &saved_without), "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"<p:cNvPr id="5" name="Note"/>"#));
    assert!(xml.contains(r#"sz="1400""#) && xml.contains("<a:t>Notes </a:t>"));
    assert!(xml.contains("<a:t>1</a:t>") && !xml.contains("slidenum"));
}

#[test]
fn a_change_of_what_a_change_before_it_removed_goes_and_the_rest_of_its_batch_stays() {
    let mut model = read(&deck());
    let removal = Change::RemoveShape {
        slide: "256".to_owned(),
        shape: "3".to_owned(),
    };
    apply_change(&mut model, removal.clone()).unwrap();
    let since = [Ordered {
        rev: 1,
        author: "ben".to_owned(),
        client: "ben".to_owned(),
        seq: 1,
        change: removal,
    }];
    let moved = |shape: &str| Change::Shape {
        slide: "256".to_owned(),
        shape: shape.to_owned(),
        fields: ShapeFields {
            x: Some(12_700),
            ..ShapeFields::default()
        },
    };
    let batch = Change::Group {
        changes: vec![moved("3"), moved("2")],
    };
    let applied = Pptx::apply(&mut model, batch, &since, &mut Assets::default()).unwrap();
    assert_eq!(
        applied,
        Change::Group {
            changes: vec![moved("2")]
        }
    );
    assert_eq!(model.slides[0].shapes[0].x, 12_700);
    assert!(matches!(
        Pptx::apply(&mut model, moved("77"), &since, &mut Assets::default()),
        Err(Refused::Invalid(_))
    ));
}

fn framed_deck() -> Vec<u8> {
    let group = concat!(
        r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="5" name="Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>"#,
        r#"<p:grpSpPr><a:xfrm><a:off x="1000" y="2000"/><a:ext cx="3000" cy="4000"/>"#,
        r#"<a:chOff x="1000" y="2000"/><a:chExt cx="3000" cy="4000"/></a:xfrm></p:grpSpPr>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="6" name="Inner"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>"#,
        r#"<a:xfrm><a:off x="1000" y="2000"/><a:ext cx="3000" cy="4000"/></a:xfrm>"#,
        r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:sp></p:grpSp>"#,
    );
    let table = concat!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="7" name="Table"/><p:cNvGraphicFramePr/>"#,
        r#"<p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="5000" y="6000"/><a:ext cx="7000" cy="8000"/>"#,
        r#"</p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">"#,
        r#"<a:tbl><a:tblGrid><a:gridCol w="7000"/></a:tblGrid><a:tr h="8000"><a:tc><a:txBody><a:bodyPr/>"#,
        r#"<a:p><a:r><a:t>cell</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic>"#,
        r#"</p:graphicFrame>"#,
    );
    let chart = concat!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="8" name="Chart"/><p:cNvGraphicFramePr/>"#,
        r#"<p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="9000" y="10000"/><a:ext cx="11000" cy="12000"/>"#,
        r#"</p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">"#,
        r#"<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId9"/>"#,
        r#"</a:graphicData></a:graphic></p:graphicFrame>"#,
    );
    let slide =
        slide_xml("First").replace("</p:spTree>", &format!("{group}{table}{chart}</p:spTree>"));
    with_parts(&deck(), &[("ppt/slides/slide1.xml", &slide)])
}

#[test]
fn a_move_of_a_group_a_table_or_a_chart_reaches_the_file_and_comes_back_with_it() {
    let file = framed_deck();
    let mut model = read(&file);
    let moved = |shape: &str| Op::MoveShape {
        slide: "256".to_owned(),
        shape: shape.to_owned(),
        x: Some(1.0),
        y: Some(2.0),
        width: None,
        height: None,
    };
    apply_ops(&mut model, vec![moved("5"), moved("7"), moved("8")]);
    let with_moves = saved(&model, &file);
    for shape in &read(&with_moves).slides[0].shapes[2..] {
        assert_eq!((shape.x, shape.y), (12_700, 25_400), "{}", shape.name);
    }
    let group = concat!(
        r#"<a:xfrm><a:off x="12700" y="25400"/><a:ext cx="3000" cy="4000"/>"#,
        r#"<a:chOff x="1000" y="2000"/><a:chExt cx="3000" cy="4000"/></a:xfrm>"#,
    );
    assert!(part(&with_moves, "ppt/slides/slide1.xml").contains(group));
    let moved_group = Pptx::snapshot(&model).0["slides"][0]["shapes"][2].clone();
    apply_ops(
        &mut model,
        vec![Op::RemoveShape {
            slide: "256".to_owned(),
            shape: "5".to_owned(),
        }],
    );
    let without = saved(&model, &with_moves);
    let back = Change::AddShape {
        slide: "256".to_owned(),
        shape: moved_group,
        keep: BTreeMap::new(),
    };
    apply_change(&mut model, back).unwrap();
    assert!(part(&saved(&model, &without), "ppt/slides/slide1.xml").contains(group));
}

#[test]
fn a_group_without_a_frame_spans_its_shapes_and_a_move_takes_them_along() {
    let group = concat!(
        r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="5" name="Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="6" name="Left"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>"#,
        r#"<a:xfrm><a:off x="1000" y="2000"/><a:ext cx="3000" cy="4000"/></a:xfrm></p:spPr></p:sp>"#,
        r#"<p:sp><p:nvSpPr><p:cNvPr id="7" name="Right"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>"#,
        r#"<a:xfrm><a:off x="6000" y="1000"/><a:ext cx="2000" cy="2000"/></a:xfrm></p:spPr></p:sp></p:grpSp>"#,
    );
    let chart = concat!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="8" name="Chart"/><p:cNvGraphicFramePr/>"#,
        r#"<p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="9000" y="10000"/></p:xfrm><a:graphic>"#,
        r#"<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">"#,
        r#"<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId9"/>"#,
        r#"</a:graphicData></a:graphic></p:graphicFrame>"#,
    );
    let slide = slide_xml("First").replace("</p:spTree>", &format!("{group}{chart}</p:spTree>"));
    let file = with_parts(&deck(), &[("ppt/slides/slide1.xml", &slide)]);
    let mut model = read(&file);
    let frame = |model: &Model, at: usize| {
        let shape = &model.slides[0].shapes[at];
        (shape.x, shape.y, shape.width, shape.height)
    };
    assert_eq!(frame(&model, 2), (1000, 1000, 7000, 5000));
    let moved = |shape: &str| Op::MoveShape {
        slide: "256".to_owned(),
        shape: shape.to_owned(),
        x: Some(1.0),
        y: Some(2.0),
        width: None,
        height: None,
    };
    apply_ops(&mut model, vec![moved("5"), moved("8")]);
    let with_moves = saved(&model, &file);
    assert_eq!(frame(&read(&with_moves), 2), (12_700, 25_400, 7000, 5000));
    let xml = part(&with_moves, "ppt/slides/slide1.xml");
    assert!(xml.contains(concat!(
        r#"<p:grpSpPr><a:xfrm><a:off x="12700" y="25400"/><a:ext cx="7000" cy="5000"/>"#,
        r#"<a:chOff x="1000" y="1000"/><a:chExt cx="7000" cy="5000"/></a:xfrm></p:grpSpPr>"#,
    )));
    assert!(xml.contains(r#"<p:xfrm><a:off x="12700" y="25400"/><a:ext cx="0" cy="0"/></p:xfrm>"#));
}

#[test]
fn a_fill_goes_into_properties_that_hold_nothing_and_a_table_or_a_chart_takes_none() {
    let file = framed_deck();
    let mut model = read(&file);
    apply_ops(
        &mut model,
        vec![
            Op::SetFill {
                slide: "256".to_owned(),
                shape: "2".to_owned(),
                color: "00FF00".to_owned(),
            },
            Op::MoveShape {
                slide: "256".to_owned(),
                shape: "2".to_owned(),
                x: Some(1.0),
                y: Some(2.0),
                width: Some(3.0),
                height: Some(4.0),
            },
        ],
    );
    let filled = saved(&model, &file);
    assert!(part(&filled, "ppt/slides/slide1.xml").contains(concat!(
        r#"<p:spPr><a:xfrm><a:off x="12700" y="25400"/><a:ext cx="38100" cy="50800"/></a:xfrm>"#,
        r#"<a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></p:spPr>"#,
    )));
    assert_eq!(
        read(&filled).slides[0].shapes[0].fill.as_deref(),
        Some("00FF00")
    );
    for graphic in ["7", "8"] {
        let colored = Change::Ops {
            ops: vec![Op::SetFill {
                slide: "256".to_owned(),
                shape: graphic.to_owned(),
                color: "00FF00".to_owned(),
            }],
        };
        assert!(matches!(
            apply_change(&mut model, colored),
            Err(Refused::Invalid(_))
        ));
    }
}
