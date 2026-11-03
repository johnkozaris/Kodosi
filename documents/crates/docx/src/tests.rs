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

#[test]
fn a_decision_keeps_one_side_of_each_tracked_change() {
    let file = docx_file(THREE);
    let tracked = |model: &mut Model| {
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
        applied(model, change).unwrap();
    };
    let decided = |model: &mut Model, op: Op| {
        let change = Change::Ops {
            ops: vec![op],
            track: false,
            author: "Ana".to_owned(),
        };
        applied(model, change).unwrap();
        let again = read(&saved(model, &file));
        assert_eq!(Docx::query(&again, Query::Check).0["trackedChanges"], 0);
        again.doc.text_content()
    };
    let mut accepted = read(&file);
    tracked(&mut accepted);
    let text = decided(&mut accepted, Op::AcceptChanges { block: None });
    assert!(text.contains("Final") && !text.contains("Last"));
    let mut rejected = read(&file);
    tracked(&mut rejected);
    let text = decided(&mut rejected, Op::RejectChanges { block: None });
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
fn hostile_files_are_refused() {
    assert!(Docx::read(b"not a zip", &mut Assets::default()).is_err());
    let doctype = docx_file("").repeat(1);
    assert!(Docx::read(&doctype, &mut Assets::default()).is_ok());
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
