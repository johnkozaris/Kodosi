use std::{collections::BTreeMap, io::Write as _, path::Path};

use document::{Assets, Format, Ordered, Refused};
use package::Package;
use serde_json::{Value, json};

use crate::{Change, Docx, Model, Op, Query, model::schema};

const WORD_MAIN: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn docx_file(body: &str) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{WORD_MAIN}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#
    );
    let relations = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", document.as_str()),
        ("word/_rels/document.xml.rels", relations),
        ("word/settings.xml", "<settings   keep=\"exact bytes\"/>"),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn read(file: &[u8]) -> Model {
    Docx::read(file, &mut Assets::default()).unwrap()
}

fn part(file: &[u8], name: &str) -> String {
    Package::open(file).unwrap().text(name).unwrap()
}

fn applied(model: &mut Model, change: Change) -> Result<Change, Refused> {
    Docx::apply(model, change, &[], &mut Assets::default())
}

fn steps_of(model: &mut Model, ops: Vec<Op>) -> Vec<Value> {
    let change = Change::Ops {
        ops,
        track: false,
        author: String::new(),
    };
    match applied(model, change).unwrap() {
        Change::Steps { steps, .. } => steps,
        other => panic!("{other:?}"),
    }
}

fn saved(model: &Model, file: &[u8]) -> Vec<u8> {
    Docx::write(model, file, &Assets::default()).unwrap()
}

fn blocks(model: &Model) -> Vec<Value> {
    let answer = Docx::query(model, Query::Read { block: None });
    answer.0["blocks"].as_array().unwrap().clone()
}

fn first_id(model: &Model) -> String {
    blocks(model)[0]["id"].as_str().unwrap().to_owned()
}

fn stepped(model: &mut Model, steps: Vec<Value>) {
    let change = Change::Steps {
        steps,
        touched: Vec::new(),
        keep: BTreeMap::default(),
        sources: BTreeMap::default(),
    };
    applied(model, change).unwrap();
}

fn starts(model: &Model) -> Vec<usize> {
    let mut position = 0;
    model
        .doc
        .content()
        .children()
        .iter()
        .map(|block| {
            let start = position;
            position += block.node_size();
            start
        })
        .collect()
}

fn attrs_of(model: &Model, name: &str) -> Vec<Value> {
    let doc = model.doc.to_json(schema());
    doc["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["attrs"][name].clone())
        .collect()
}

fn mark_names(node: &Value) -> Vec<&str> {
    node["marks"]
        .as_array()
        .map(|marks| {
            marks
                .iter()
                .map(|mark| mark["type"].as_str().unwrap())
                .collect()
        })
        .unwrap_or_default()
}

const THREE: &str = r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p><w:p w:rsidR="00AB"><w:r><w:rPr><w:rFonts w:ascii="Arial"/><w:b/><w:lang w:val="el-GR"/></w:rPr><w:t xml:space="preserve">bold words </w:t></w:r><w:r><w:t>and plain</w:t></w:r><w:bookmarkStart w:id="1" w:name="x"/></w:p><w:p><w:r><w:t>Last</w:t></w:r></w:p>"#;

#[test]
fn no_edit_means_the_same_bytes() {
    let file = docx_file(THREE);
    let model = read(&file);
    assert_eq!(saved(&model, &file), file);
}

#[test]
fn the_tree_holds_blocks_with_their_text_and_look() {
    let model = read(&docx_file(THREE));
    let all = blocks(&model);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0]["style"], "Heading1");
    assert_eq!(all[1]["text"], "bold words and plain");
    let doc = model.doc.to_json(schema());
    let run = &doc["content"][1]["content"][0];
    let marks: Vec<&str> = run["marks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mark| mark["type"].as_str().unwrap())
        .collect();
    assert_eq!(marks, ["run", "bold", "textStyle"]);
}

#[test]
fn one_edit_changes_one_block_and_nothing_else() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let last = blocks(&model)[2]["id"].as_str().unwrap().to_owned();
    steps_of(
        &mut model,
        vec![Op::ReplaceText {
            find: "Last".to_owned(),
            with: "Final".to_owned(),
            block: Some(last),
            all: false,
        }],
    );
    let new_file = saved(&model, &file);
    let old_xml = part(&file, "word/document.xml");
    let new_xml = part(&new_file, "word/document.xml");
    assert_eq!(
        new_xml,
        old_xml.replace(
            "<w:p><w:r><w:t>Last</w:t></w:r></w:p>",
            r#"<w:p><w:r><w:t xml:space="preserve">Final</w:t></w:r></w:p>"#
        )
    );
    assert_eq!(
        part(&new_file, "word/settings.xml"),
        part(&file, "word/settings.xml")
    );
}

#[test]
fn an_edit_keeps_what_the_tree_does_not_show() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    steps_of(
        &mut model,
        vec![Op::ReplaceText {
            find: "bold words".to_owned(),
            with: "heavy text".to_owned(),
            block: None,
            all: false,
        }],
    );
    let xml = part(&saved(&model, &file), "word/document.xml");
    assert!(xml.contains(r#"<w:rFonts w:ascii="Arial"/><w:b/><w:lang w:val="el-GR"/>"#));
    assert!(xml.contains(r#"<w:bookmarkStart w:id="1" w:name="x"/>"#));
    assert!(xml.contains(">heavy text "));
}

#[test]
fn a_new_look_keeps_the_other_properties_in_schema_order() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    steps_of(
        &mut model,
        vec![Op::FormatText {
            find: "bold words".to_owned(),
            block: None,
            bold: false,
            italic: true,
            underline: false,
            color: None,
        }],
    );
    let xml = part(&saved(&model, &file), "word/document.xml");
    assert!(xml.contains(r#"<w:rFonts w:ascii="Arial"/><w:b/><w:i/><w:lang w:val="el-GR"/>"#));
}

#[test]
fn a_view_can_send_steps_and_the_saved_file_reads_the_same() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let step = json!({
        "stepType": "replace",
        "from": 2,
        "to": 2,
        "slice": { "content": [{ "type": "text", "text": "A " }] },
    });
    let change = Change::Steps {
        steps: vec![step],
        touched: Vec::new(),
        keep: BTreeMap::default(),
        sources: BTreeMap::default(),
    };
    match applied(&mut model, change).unwrap() {
        Change::Steps { touched, .. } => assert_eq!(touched, [first_id(&model)]),
        other => panic!("{other:?}"),
    }
    let new_file = saved(&model, &file);
    let again = read(&new_file);
    assert_eq!(blocks(&again)[0]["text"], "TA itle");
    assert_eq!(blocks(&again)[0]["style"], "Heading1");
}

#[test]
fn a_place_moves_with_steps_before_it_and_reads_as_its_block_and_words() {
    let mut model = read(&docx_file(THREE));
    let second = blocks(&model)[1]["id"].clone();
    let plain = 8 + "bold words and ".len();
    let mut place = json!({ "from": plain, "to": plain + "plain".len() });
    let change = Change::Steps {
        steps: vec![json!({
            "stepType": "replace",
            "from": 2,
            "to": 2,
            "slice": { "content": [{ "type": "text", "text": "A " }] },
        })],
        touched: Vec::new(),
        keep: BTreeMap::default(),
        sources: BTreeMap::default(),
    };
    let change = applied(&mut model, change).unwrap();
    Docx::map_place(&mut place, &change);
    assert_eq!(place, json!({ "from": plain + 2, "to": plain + 7 }));
    assert_eq!(
        Docx::describe_place(&model, &place),
        json!({ "block": second, "selected": "plain" })
    );
    let caret = json!({ "from": plain + 2, "to": plain + 2 });
    assert_eq!(
        Docx::describe_place(&model, &caret),
        json!({ "block": second })
    );
}

#[test]
fn old_steps_are_behind_and_old_ops_only_when_they_meet() {
    let mut model = read(&docx_file(THREE));
    let first = blocks(&model)[0]["id"].as_str().unwrap().to_owned();
    let last = blocks(&model)[2]["id"].as_str().unwrap().to_owned();
    let earlier = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: Change::Steps {
            steps: Vec::new(),
            touched: vec![first.clone()],
            keep: BTreeMap::default(),
            sources: BTreeMap::default(),
        },
    };
    let since = [earlier];
    let steps = Change::Steps {
        steps: Vec::new(),
        touched: Vec::new(),
        keep: BTreeMap::default(),
        sources: BTreeMap::default(),
    };
    let result = Docx::apply(&mut model, steps, &since, &mut Assets::default());
    assert!(matches!(result, Err(Refused::Behind(_))));

    let same_place = Change::Ops {
        ops: vec![Op::SetStyle {
            block: first,
            style: None,
        }],
        track: false,
        author: String::new(),
    };
    let result = Docx::apply(&mut model, same_place, &since, &mut Assets::default());
    match result {
        Err(Refused::Behind(detail)) => assert!(detail.contains("Ana")),
        other => panic!("{other:?}"),
    }

    let other_place = Change::Ops {
        ops: vec![Op::SetStyle {
            block: last,
            style: Some("Quote".to_owned()),
        }],
        track: false,
        author: String::new(),
    };
    assert!(Docx::apply(&mut model, other_place, &since, &mut Assets::default()).is_ok());
}

#[test]
fn ops_are_all_or_none() {
    let mut model = read(&docx_file(THREE));
    let before = model.doc.clone();
    let change = Change::Ops {
        ops: vec![
            Op::InsertParagraph {
                text: "new".to_owned(),
                after: None,
                style: None,
            },
            Op::DeleteBlock {
                block: "nothing".to_owned(),
            },
        ],
        track: false,
        author: String::new(),
    };
    assert!(matches!(
        applied(&mut model, change),
        Err(Refused::Invalid(_))
    ));
    assert_eq!(model.doc, before);
}

#[test]
fn tracked_edits_become_tracked_changes_in_the_file() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let change = Change::Ops {
        ops: vec![Op::ReplaceText {
            find: "Last".to_owned(),
            with: "Final".to_owned(),
            block: None,
            all: false,
        }],
        track: true,
        author: "Agent".to_owned(),
    };
    applied(&mut model, change).unwrap();
    let xml = part(&saved(&model, &file), "word/document.xml");
    assert!(xml.contains(r#"w:author="Agent""#));
    assert!(xml.contains(r#"<w:delText xml:space="preserve">Last</w:delText>"#));
    assert!(xml.contains("<w:ins "));
    let again = read(&saved(&model, &file));
    assert_eq!(Docx::query(&again, Query::Check).0["trackedChanges"], 2);
}

fn tracked(model: &mut Model, ops: Vec<Op>) -> Result<Change, Refused> {
    let change = Change::Ops {
        ops,
        track: true,
        author: "Agent".to_owned(),
    };
    applied(model, change)
}

fn decided(model: &Model, file: &[u8], op: Op) -> Model {
    let mut model = model.clone();
    let change = Change::Ops {
        ops: vec![op],
        track: false,
        author: "Ana".to_owned(),
    };
    applied(&mut model, change).unwrap();
    let again = read(&saved(&model, file));
    assert_eq!(Docx::query(&again, Query::Check).0["trackedChanges"], 0);
    again
}

#[test]
fn a_decision_keeps_one_side_of_each_tracked_change() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    tracked(
        &mut model,
        vec![Op::ReplaceText {
            find: "Last".to_owned(),
            with: "Final".to_owned(),
            block: None,
            all: false,
        }],
    )
    .unwrap();
    let text = decided(&model, &file, Op::AcceptChanges { block: None })
        .doc
        .text_content();
    assert!(text.contains("Final") && !text.contains("Last"));
    let mut rejected = decided(&model, &file, Op::RejectChanges { block: None });
    let text = rejected.doc.text_content();
    assert!(text.contains("Last") && !text.contains("Final"));
    assert!(matches!(
        applied(
            &mut rejected,
            Change::Ops {
                ops: vec![Op::AcceptChanges { block: None }],
                track: false,
                author: "Ana".to_owned(),
            }
        ),
        Err(Refused::Invalid(_))
    ));
}

#[test]
fn a_tracked_paragraph_is_an_insertion_of_its_words_and_of_its_mark() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let first = first_id(&model);
    tracked(
        &mut model,
        vec![Op::InsertParagraph {
            text: "Added".to_owned(),
            after: Some(first),
            style: Some("ListParagraph".to_owned()),
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    assert!(part(&new_file, "word/document.xml").contains(concat!(
        r#"<w:p><w:pPr><w:pStyle w:val="ListParagraph"/><w:rPr><w:ins w:id="90000" w:author="Agent"/></w:rPr></w:pPr>"#,
        r#"<w:ins w:id="90001" w:author="Agent"><w:r><w:t xml:space="preserve">Added</w:t></w:r></w:ins></w:p>"#,
    )));
    let again = read(&new_file);
    assert_eq!(Docx::query(&again, Query::Check).0["trackedChanges"], 2);
    let accepted = decided(&again, &new_file, Op::AcceptChanges { block: None });
    assert_eq!(blocks(&accepted)[1]["text"], "Added");
    assert_eq!(blocks(&accepted)[1]["style"], "ListParagraph");
    let rejected = decided(&again, &new_file, Op::RejectChanges { block: None });
    assert_eq!(blocks(&rejected), blocks(&read(&file)));
}

#[test]
fn a_tracked_deletion_of_a_paragraph_strikes_its_words_and_its_mark() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let last = blocks(&model)[2]["id"].as_str().unwrap().to_owned();
    tracked(&mut model, vec![Op::DeleteBlock { block: last }]).unwrap();
    let new_file = saved(&model, &file);
    assert!(part(&new_file, "word/document.xml").contains(concat!(
        r#"<w:p><w:pPr><w:rPr><w:del w:id="90000" w:author="Agent"/></w:rPr></w:pPr>"#,
        r#"<w:del w:id="90001" w:author="Agent"><w:r><w:delText xml:space="preserve">Last</w:delText></w:r></w:del></w:p>"#,
    )));
    let again = read(&new_file);
    let accepted = decided(&again, &new_file, Op::AcceptChanges { block: None });
    assert_eq!(blocks(&accepted).len(), 2);
    let rejected = decided(&again, &new_file, Op::RejectChanges { block: None });
    assert_eq!(blocks(&rejected)[2]["text"], "Last");
}

#[test]
fn an_accepted_deletion_of_a_paragraph_leaves_what_a_direct_deletion_leaves() {
    let file = docx_file(concat!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="0" w:name="_Toc1"/>"#,
        r#"<w:r><w:lastRenderedPageBreak/><w:t xml:space="preserve">Old </w:t></w:r><w:r><w:t>heading</w:t></w:r>"#,
        r#"<w:bookmarkEnd w:id="0"/></w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Body text</w:t></w:r></w:p>"#,
    ));
    let mut direct = read(&file);
    let heading = first_id(&direct);
    steps_of(
        &mut direct,
        vec![Op::DeleteBlock {
            block: heading.clone(),
        }],
    );
    let mut model = read(&file);
    tracked(&mut model, vec![Op::DeleteBlock { block: heading }]).unwrap();
    let suggested = saved(&model, &file);
    assert!(
        part(&suggested, "word/document.xml")
            .contains(r#"<w:delText xml:space="preserve">Old heading</w:delText>"#)
    );
    let accepted = decided(
        &read(&suggested),
        &suggested,
        Op::AcceptChanges { block: None },
    );
    assert_eq!(
        part(&saved(&accepted, &suggested), "word/document.xml"),
        part(&saved(&direct, &file), "word/document.xml")
    );
}

#[test]
fn a_saved_suggestion_reads_as_the_paragraphs_that_made_it() {
    let file = docx_file(concat!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:spacing w:after="120"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>Last</w:t></w:r></w:p>"#,
    ));
    let mut model = read(&file);
    let title = first_id(&model);
    tracked(
        &mut model,
        vec![
            Op::InsertParagraph {
                text: "Added".to_owned(),
                after: None,
                style: Some("ListParagraph".to_owned()),
            },
            Op::DeleteBlock { block: title },
        ],
    )
    .unwrap();
    let changes = Docx::compare(&model, &read(&saved(&model, &file)));
    assert!(changes.is_empty(), "{changes:?}");
}

#[test]
fn a_tracked_cell_keeps_its_old_text_until_a_person_decides() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    steps_of(
        &mut model,
        vec![Op::InsertTable {
            rows: vec![vec!["a".to_owned(), "b".to_owned()]],
            after: None,
        }],
    );
    let table = blocks(&model)[3]["id"].as_str().unwrap().to_owned();
    tracked(
        &mut model,
        vec![Op::SetCell {
            table,
            row: 0,
            column: 1,
            text: "B".to_owned(),
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    let again = read(&new_file);
    assert_eq!(Docx::query(&again, Query::Check).0["trackedChanges"], 2);
    let rows = |model: &Model| blocks(model)[3]["rows"].clone();
    let accepted = decided(&again, &new_file, Op::AcceptChanges { block: None });
    assert_eq!(rows(&accepted), json!([["a", "B"]]));
    let rejected = decided(&again, &new_file, Op::RejectChanges { block: None });
    assert_eq!(rows(&rejected), json!([["a", "b"]]));
}

#[test]
fn a_look_or_a_table_changes_directly_under_track() {
    let mut model = read(&docx_file(THREE));
    let first = first_id(&model);
    tracked(
        &mut model,
        vec![
            Op::SetStyle {
                block: first,
                style: Some("Quote".to_owned()),
            },
            Op::InsertTable {
                rows: vec![vec!["a".to_owned()]],
                after: None,
            },
        ],
    )
    .unwrap();
    assert_eq!(blocks(&model)[0]["style"], "Quote");
    let table = blocks(&model)[3]["id"].as_str().unwrap().to_owned();
    tracked(&mut model, vec![Op::DeleteBlock { block: table }]).unwrap();
    assert_eq!(blocks(&model).len(), 3);
    assert_eq!(Docx::query(&model, Query::Check).0["trackedChanges"], 0);
}

#[test]
fn a_mark_that_word_tracked_alone_is_a_change_and_its_rejection_joins_the_paragraphs_in_the_next() {
    let file = docx_file(concat!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:rPr><w:ins w:id="1" w:author="Ana" w:date="2026-11-09T10:00:00Z"/></w:rPr></w:pPr>"#,
        r#"<w:r><w:t xml:space="preserve">Hello </w:t></w:r></w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>world</w:t></w:r></w:p>"#,
    ));
    let model = read(&file);
    assert_eq!(saved(&model, &file), file);
    assert_eq!(Docx::query(&model, Query::Check).0["trackedChanges"], 1);
    let rejected = decided(&model, &file, Op::RejectChanges { block: None });
    let all = blocks(&rejected);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["text"], "Hello world");
    assert_eq!(all[0]["style"], Value::Null);
    assert_eq!(attrs_of(&rejected, "align"), [json!("center")]);
}

#[test]
fn a_paragraph_a_table_and_a_cell_can_be_added() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    steps_of(
        &mut model,
        vec![
            Op::InsertParagraph {
                text: "Added".to_owned(),
                after: None,
                style: Some("Heading2".to_owned()),
            },
            Op::InsertTable {
                rows: vec![vec!["a".to_owned(), "b".to_owned()]],
                after: None,
            },
        ],
    );
    let table = blocks(&model).last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    steps_of(
        &mut model,
        vec![Op::SetCell {
            table,
            row: 0,
            column: 1,
            text: "B".to_owned(),
        }],
    );
    let new_file = saved(&model, &file);
    let again = read(&new_file);
    let all = blocks(&again);
    assert_eq!(all[3]["style"], "Heading2");
    assert_eq!(all[4]["rows"], json!([["a", "B"]]));
}

#[test]
fn text_with_characters_that_xml_cannot_hold_saves_a_file_that_opens() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    steps_of(
        &mut model,
        vec![Op::InsertParagraph {
            text: "End of page 1\u{c}Page 2 \u{1b}[1mbold\u{1b}[0m\u{b}".to_owned(),
            after: None,
            style: None,
        }],
    );
    let again = read(&saved(&model, &file));
    assert_eq!(blocks(&again)[3]["text"], "End of page 1Page 2 [1mbold[0m");
}

#[test]
fn a_compare_finds_only_what_a_write_changed() {
    let file = docx_file(THREE);
    let base = read(&file);
    let written = docx_file(
        &THREE
            .replace("Title", "New title")
            .replace("Last", "Last</w:t></w:r></w:p><w:p><w:r><w:t>More"),
    );
    let changes = Docx::compare(&base, &read(&written));
    let [Change::Merge { ops, .. }] = changes.as_slice() else {
        panic!("{changes:?}");
    };
    assert_eq!(ops.len(), 2);
}

#[test]
fn a_merge_keeps_a_persons_edit_and_the_writers_bytes() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let last = blocks(&model)[2]["id"].as_str().unwrap().to_owned();
    steps_of(
        &mut model,
        vec![Op::ReplaceText {
            find: "Last".to_owned(),
            with: "Person".to_owned(),
            block: Some(last),
            all: false,
        }],
    );
    let written = docx_file(&THREE.replace("Title", "From script"));
    let base = read(&file);
    let writer = read(&written);
    for change in Docx::compare(&base, &writer) {
        applied(&mut model, change).unwrap();
    }
    let xml = part(&saved(&model, &written), "word/document.xml");
    assert!(xml.contains("From script"));
    assert!(xml.contains("Person"));
    assert!(
        xml.contains(r#"<w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>From script</w:t></w:r>"#)
    );
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = Docx::guide();
    for op in &guide.ops {
        let mut example = json!({ "op": op.name });
        for field in op.fields.iter().filter(|field| field.required) {
            example[&field.name] = match field.kind.as_str() {
                "number" => json!(0),
                "rows of text" => json!([["a"]]),
                _ => json!("x"),
            };
        }
        let parsed: Result<Op, _> = serde_json::from_value(example);
        assert!(parsed.is_ok(), "{}: {parsed:?}", op.name);
    }
}

#[test]
fn a_file_that_is_no_package_is_refused() {
    assert!(Docx::read(b"not a zip", &mut Assets::default()).is_err());
}

#[test]
fn genoffices_samples_read_and_keep_each_byte() {
    let folder = std::env::var_os("GENOFFICE_FIXTURES");
    let Some(folder) = folder else {
        return;
    };
    for entry in std::fs::read_dir(Path::new(&folder)).unwrap().flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "docx")
        {
            let file = std::fs::read(&path).unwrap();
            let model = read(&file);
            assert_eq!(saved(&model, &file), file, "{}", path.display());
        }
    }
}

#[test]
fn a_new_file_from_markdown_has_bullets_and_numbers() {
    let model = read(&crate::new_file(Some("- one\n2. two")).unwrap());
    let snapshot = Docx::snapshot(&model).0;
    let lists = &snapshot["view"]["numbering"];
    assert_eq!(lists["1"][0]["format"], "bullet");
    assert_eq!(lists["2"][0]["format"], "decimal");
    assert_eq!(lists["2"][1]["text"], "%2.");
    let items: Vec<&Value> = snapshot["doc"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| &block["attrs"]["numId"])
        .collect();
    assert_eq!(items, [&json!("1"), &json!("2")]);
}

#[test]
fn superscript_and_subscript_are_the_vertical_alignment_of_a_run() {
    let file = docx_file(
        r#"<w:p><w:r><w:t>E=mc</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>2</w:t></w:r></w:p>"#,
    );
    let mut model = read(&file);
    let doc = model.doc.to_json(schema());
    assert_eq!(
        mark_names(&doc["content"][0]["content"][1]),
        ["run", "superscript"]
    );
    stepped(
        &mut model,
        vec![
            json!({ "stepType": "addMark", "from": 5, "to": 6, "mark": { "type": "subscript" } }),
            json!({ "stepType": "addMark", "from": 1, "to": 2, "mark": { "type": "superscript" } }),
        ],
    );
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "word/document.xml");
    assert!(xml.contains(
        r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t xml:space="preserve">E</w:t></w:r>"#
    ));
    assert!(xml.contains(
        r#"<w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:t xml:space="preserve">2</w:t></w:r>"#
    ));
    let doc = read(&new_file).doc.to_json(schema());
    let runs = doc["content"][0]["content"].as_array().unwrap();
    assert_eq!(mark_names(&runs[0]), ["run", "superscript"]);
    assert_eq!(mark_names(runs.last().unwrap()), ["run", "subscript"]);
}

#[test]
fn line_spacing_comes_and_goes_and_the_space_around_a_paragraph_stays() {
    let file = docx_file(concat!(
        r#"<w:p><w:pPr><w:spacing w:before="120" w:after="240" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:t>One</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:spacing w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>Two</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>Three</w:t></w:r></w:p>"#,
    ));
    let mut model = read(&file);
    assert_eq!(
        attrs_of(&model, "line"),
        [json!("276"), Value::Null, Value::Null]
    );
    let steps = starts(&model)
        .into_iter()
        .map(|pos| json!({ "stepType": "attr", "pos": pos, "attr": "line", "value": "480" }))
        .collect();
    stepped(&mut model, steps);
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "word/document.xml");
    assert!(
        xml.contains(r#"<w:spacing w:line="480" w:lineRule="auto" w:before="120" w:after="240"/>"#)
    );
    assert!(!xml.contains("exact"));
    assert_eq!(attrs_of(&read(&new_file), "line"), vec![json!("480"); 3]);
}

#[test]
fn a_left_indent_moves_and_the_rest_of_the_indent_stays() {
    let file = docx_file(concat!(
        r#"<w:p><w:pPr><w:ind w:left="720" w:leftChars="200" w:hanging="360"/></w:pPr><w:r><w:t>One</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:ind w:start="1440" w:end="720"/></w:pPr><w:r><w:t>Two</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>Three</w:t></w:r></w:p>"#,
    ));
    let mut model = read(&file);
    assert_eq!(
        attrs_of(&model, "indent"),
        [json!("720"), json!("1440"), Value::Null]
    );
    let steps = starts(&model)
        .into_iter()
        .zip(["1440", "720", "720"])
        .map(|(pos, indent)| {
            json!({ "stepType": "attr", "pos": pos, "attr": "indent", "value": indent })
        })
        .collect();
    stepped(&mut model, steps);
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "word/document.xml");
    assert!(xml.contains(r#"<w:ind w:left="1440" w:hanging="360"/>"#));
    assert!(xml.contains(r#"<w:ind w:start="720" w:end="720"/>"#));
    assert!(xml.contains(r#"<w:pPr><w:ind w:left="720"/></w:pPr>"#));
    assert_eq!(
        attrs_of(&read(&new_file), "indent"),
        [json!("1440"), json!("720"), json!("720")]
    );
}

#[test]
fn a_page_break_is_a_break_in_a_run_of_its_own() {
    let file = docx_file(
        r#"<w:p><w:r><w:t>One</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p>"#,
    );
    let mut model = read(&file);
    assert_eq!(saved(&model, &file), file);
    let end = starts(&model)[1] + 1 + "Two".len();
    stepped(
        &mut model,
        vec![json!({
            "stepType": "replace",
            "from": end,
            "to": end,
            "slice": { "content": [{ "type": "pageBreak" }] },
        })],
    );
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "word/document.xml");
    assert!(xml.contains(
        r#"<w:t xml:space="preserve">Two</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>"#
    ));
    let doc = read(&new_file).doc.to_json(schema());
    for paragraph in doc["content"].as_array().unwrap() {
        assert_eq!(paragraph["content"][1]["type"], "pageBreak");
    }
}

#[test]
fn a_run_with_a_child_that_the_tree_does_not_hold_stays_whole() {
    let file = docx_file(
        r#"<w:p><w:r><w:t>Hi</w:t><w:sym w:font="Wingdings" w:char="F04A"/></w:r></w:p>"#,
    );
    let mut model = read(&file);
    stepped(
        &mut model,
        vec![json!({
            "stepType": "replace",
            "from": 1,
            "to": 1,
            "slice": { "content": [{ "type": "text", "text": "Oh " }] },
        })],
    );
    let xml = part(&saved(&model, &file), "word/document.xml");
    assert_eq!(xml.matches("Hi").count(), 1);
    assert!(xml.contains(r#"<w:r><w:t>Hi</w:t><w:sym w:font="Wingdings" w:char="F04A"/></w:r>"#));
}

#[test]
fn a_run_with_words_and_pictures_keeps_each_once_through_saves() {
    let picture = |id: u32| {
        format!(
            concat!(
                r#"<w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">"#,
                r#"<wp:extent cx="952500" cy="952500"/><wp:docPr id="{id}" name="Picture {id}"/>"#,
                r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
                r#"<a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId9"/>"#,
                r#"</a:graphicData></a:graphic></wp:inline></w:drawing>"#,
            ),
            id = id
        )
    };
    let body = format!(
        r#"<w:p><w:r w:rsidR="00AB"><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Figure 1: </w:t>{}{}</w:r></w:p><w:p><w:r><w:t>Next</w:t></w:r></w:p>"#,
        picture(1),
        picture(2)
    );
    let relations = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let parts = package::Parts::from([
        (
            "word/_rels/document.xml.rels".to_owned(),
            Some(relations.as_bytes().to_vec()),
        ),
        (
            "word/media/image1.png".to_owned(),
            Some(b"picture".to_vec()),
        ),
    ]);
    let mut file = package::rewrite(&docx_file(&body), &parts).unwrap();
    for style in ["Caption", "Normal"] {
        let mut model = read(&file);
        let block = first_id(&model);
        steps_of(
            &mut model,
            vec![Op::SetStyle {
                block,
                style: Some(style.to_owned()),
            }],
        );
        file = saved(&model, &file);
    }
    let xml = part(&file, "word/document.xml");
    assert_eq!(xml.matches("Figure 1: ").count(), 1);
    assert_eq!(xml.matches("<w:drawing>").count(), 2);
    assert!(xml.contains(r#"<w:r w:rsidR="00AB"><w:rPr><w:b/></w:rPr><w:drawing>"#));
}

#[test]
fn a_new_table_has_lines_and_a_grid_as_wide_as_the_text() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    steps_of(
        &mut model,
        vec![Op::InsertTable {
            rows: vec![vec!["a".to_owned(), "b".to_owned()]],
            after: None,
        }],
    );
    let xml = part(&saved(&model, &file), "word/document.xml");
    assert!(xml.contains(r#"<w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#));
    assert!(xml.contains(
        r#"</w:tblPr><w:tblGrid><w:gridCol w:w="4513"/><w:gridCol w:w="4513"/></w:tblGrid><w:tr>"#
    ));
}

const MERGED: &str = concat!(
    r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="6000"/></w:tblGrid>"#,
    r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"<w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>E</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"</w:tbl><w:p/>"#,
);

#[test]
fn a_changed_table_keeps_its_merges_and_a_grid_column_for_each_column() {
    let file = docx_file(MERGED);
    let mut model = read(&file);
    let mut table = model.doc.to_json(schema())["content"][0].clone();
    let rows = table["content"].as_array_mut().unwrap();
    rows.remove(0);
    rows[0]["content"][0]["attrs"]["merged"] = json!(false);
    for row in &mut rows[..2] {
        let cell = json!({ "type": "tableCell", "content": [{ "type": "paragraph" }] });
        row["content"].as_array_mut().unwrap().push(cell);
    }
    rows[2]["content"][0]["attrs"]["span"] = json!(3);
    let size = model.doc.content().children()[0].node_size();
    stepped(
        &mut model,
        vec![json!({
            "stepType": "replace",
            "from": 0,
            "to": size,
            "slice": { "content": [table] },
        })],
    );
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "word/document.xml");
    assert!(xml.contains(
        r#"<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>"#
    ));
    assert!(xml.contains(
        r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vMerge w:val="restart"/></w:tcPr>"#
    ));
    assert!(xml.contains(r#"<w:tc><w:tcPr><w:gridSpan w:val="3"/></w:tcPr>"#));
    let table = &read(&new_file).doc.to_json(schema())["content"][0];
    let first = |row: usize| &table["content"][row]["content"][0]["attrs"];
    assert_eq!(first(0)["merged"], false);
    assert_eq!(first(1)["merged"], true);
    assert_eq!(first(2)["span"], 3);
}

#[test]
fn a_new_link_keeps_one_relationship_over_many_saves() {
    let file = docx_file(THREE);
    let mut model = read(&file);
    let start = starts(&model)[2] + 1;
    let link =
        json!({ "type": "link", "attrs": { "href": "https://kodosi.com", "relation": null } });
    stepped(
        &mut model,
        vec![json!({ "stepType": "addMark", "from": start, "to": start + 4, "mark": link })],
    );
    let first = saved(&model, &file);
    stepped(
        &mut model,
        vec![json!({
            "stepType": "replace",
            "from": 2,
            "to": 2,
            "slice": { "content": [{ "type": "text", "text": "A " }] },
        })],
    );
    let second = saved(&model, &first);
    let relations = part(&second, "word/_rels/document.xml.rels");
    assert_eq!(relations.matches("https://kodosi.com").count(), 1);
    assert!(relations.contains(r#"Id="rIdDoc1""#));
    assert!(part(&second, "word/document.xml").contains(r#"r:id="rIdDoc1""#));
}
