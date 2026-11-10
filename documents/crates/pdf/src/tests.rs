use document::{Assets, Format, Ordered, Refused};
use lopdf::{
    Dictionary, Document, Object, Stream,
    content::{Content, Operation},
    dictionary,
};
use serde_json::json;

use crate::{Change, Model, Op, PageWork, Pdf, Query};

fn page_content(words: &[(&str, i64, i64)]) -> Vec<u8> {
    let mut operations = Vec::new();
    for (text, x, y) in words {
        operations.push(Operation::new("BT", Vec::new()));
        operations.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
        operations.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        operations.push(Operation::new("Tj", vec![Object::string_literal(*text)]));
        operations.push(Operation::new("ET", Vec::new()));
    }
    Content { operations }.encode().unwrap()
}

fn pdf_file() -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font },
    });
    let mut kids = Vec::new();
    for words in [
        vec![("visible words", 72, 700), ("secret number", 72, 600)],
        vec![("second page", 72, 700)],
    ] {
        let content = document.add_object(Stream::new(Dictionary::new(), page_content(&words)));
        let page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages,
            "Contents" => content,
        });
        kids.push(Object::Reference(page));
    }
    let field = document.add_object(dictionary! {
        "T" => Object::string_literal("name"),
        "FT" => "Tx",
        "V" => Object::string_literal("Ana"),
    });
    let form = document.add_object(dictionary! {
        "Fields" => vec![Object::Reference(field)],
    });
    document.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => 2,
            "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages,
        "AcroForm" => form,
    });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn read(file: &[u8]) -> Model {
    Pdf::read(file, &mut Assets::default()).unwrap()
}

fn ops(model: &mut Model, ops: Vec<Op>) -> Result<Change, Refused> {
    Pdf::apply(model, Change::Ops { ops }, &[], &mut Assets::default())
}

fn saved(model: &Model, file: &[u8]) -> Vec<u8> {
    Pdf::write(model, file, &Assets::default()).unwrap()
}

fn text_of(file: &[u8], page: u32) -> String {
    Document::load_mem(file)
        .unwrap()
        .extract_text(&[page])
        .unwrap_or_default()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn no_edit_means_the_same_bytes() {
    let file = pdf_file();
    assert_eq!(saved(&read(&file), &file), file);
}

#[test]
fn pages_come_with_inherited_sizes_and_form_values() {
    let model = read(&pdf_file());
    let snapshot = Pdf::snapshot(&model).0;
    assert_eq!(snapshot["pages"].as_array().unwrap().len(), 2);
    assert_eq!(snapshot["pages"][0]["width"], 612.0);
    assert_eq!(snapshot["fields"]["name"], "Ana");
}

#[test]
fn a_note_goes_after_each_old_byte() {
    let file = pdf_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::AddNote {
            page: 1,
            text: "Look here".to_owned(),
            x: 72.0,
            y: 700.0,
            width: None,
            height: None,
            kind: None,
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    assert_eq!(&new_file[..file.len()], &file[..]);
    let again = read(&new_file);
    let note = again.notes.values().next().unwrap();
    assert_eq!(note.text, "Look here");
    assert_eq!(note.kind, "Text");
    assert_eq!(again.pages, model.pages);
}

#[test]
fn a_note_can_change_and_go() {
    let file = pdf_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::AddNote {
            page: 2,
            text: "One".to_owned(),
            x: 10.0,
            y: 10.0,
            width: Some(50.0),
            height: Some(20.0),
            kind: Some("Highlight".to_owned()),
        }],
    )
    .unwrap();
    let with_note = saved(&model, &file);
    let mut model = read(&with_note);
    let id = model.notes.keys().next().unwrap().clone();
    ops(&mut model, vec![Op::RemoveNote { note: id }]).unwrap();
    let without = saved(&model, &with_note);
    assert_eq!(read(&without).notes.len(), 0);
}

#[test]
fn a_form_value_keeps_the_old_bytes() {
    let file = pdf_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::SetField {
            name: "name".to_owned(),
            value: Some("Βίκυ".to_owned()),
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    assert_eq!(&new_file[..file.len()], &file[..]);
    assert_eq!(read(&new_file).fields["name"], "Βίκυ");
}

#[test]
fn pages_can_move_turn_and_go() {
    let file = pdf_file();
    let mut model = read(&file);
    let first = model.pages[0].id.clone();
    ops(&mut model, vec![Op::MovePage { page: 1, to: 2 }]).unwrap();
    assert_eq!(model.pages[1].id, first);
    ops(
        &mut model,
        vec![Op::RotatePage {
            page: 2,
            degrees: 90,
        }],
    )
    .unwrap();
    assert_eq!(model.pages[1].rotation, 90);
    let new_file = saved(&model, &file);
    assert!(text_of(&new_file, 1).contains("second page"));
    assert_eq!(read(&new_file).pages[1].rotation, 90);
    ops(&mut model, vec![Op::RemovePage { page: 1 }]).unwrap();
    assert_eq!(read(&saved(&model, &new_file)).pages.len(), 1);
}

#[test]
fn a_redaction_leaves_the_words_nowhere_and_seals() {
    let file = pdf_file();
    let mut model = read(&file);
    assert!(contains_bytes(&file, b"secret number"));
    let change = ops(
        &mut model,
        vec![Op::Redact {
            page: 1,
            rects: vec![[60.0, 590.0, 300.0, 620.0]],
        }],
    )
    .unwrap();
    let Change::Group { changes } = &change else {
        panic!("{change:?}");
    };
    assert!(Pdf::seals(&changes[0]));
    let new_file = saved(&model, &file);
    assert!(!contains_bytes(&new_file, b"secret number"));
    assert!(!text_of(&new_file, 1).contains("secret"));
    assert!(text_of(&new_file, 1).contains("visible words"));
    assert!(text_of(&new_file, 2).contains("second page"));
}

/// A page that draws its words through a form, as pdfpages, stamps and placed PDF figures do,
/// after these operations of its own.
fn form_pdf(drawn: &str, operations: &[u8]) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let form = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        },
        page_content(&[("visible words", 72, 700), ("secret number", 72, 600)]),
    ));
    let mut content = operations.to_vec();
    content.extend_from_slice(format!("\nq 1 0 0 1 0 0 cm /{drawn} Do Q").as_bytes());
    let content = document.add_object(Stream::new(Dictionary::new(), content));
    let page = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "Contents" => content,
        "Resources" => dictionary! { "XObject" => dictionary! { "Fm0" => form } },
    });
    document.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page)],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn redacted(model: &mut Model) -> Result<Change, Refused> {
    ops(
        model,
        vec![Op::Redact {
            page: 1,
            rects: vec![[60.0, 590.0, 300.0, 620.0]],
        }],
    )
}

#[test]
fn a_redaction_takes_the_words_out_of_a_form_and_the_pictures_in_the_page() {
    let picture = b"q 100 0 0 20 70 595 cm BI /W 1 /H 1 /CS /G /BPC 8 ID \x7f EI Q";
    let file = form_pdf("Fm0", picture);
    let mut model = read(&file);
    assert!(contains_bytes(&file, b"secret number"));
    redacted(&mut model).unwrap();
    let new_file = saved(&model, &file);
    assert!(!contains_bytes(&new_file, b"secret number"));
    assert!(contains_bytes(&new_file, b"visible words"));
    let document = Document::load_mem(&new_file).unwrap();
    let page = document.get_pages()[&1];
    let operators: Vec<String> = Content::decode(&document.get_page_content(page))
        .unwrap()
        .operations
        .into_iter()
        .map(|operation| operation.operator)
        .collect();
    assert!(operators.contains(&"Do".to_owned()));
    assert!(!operators.contains(&"BI".to_owned()));
}

#[test]
fn a_redaction_refuses_a_page_that_draws_what_it_cannot_look_into() {
    let file = form_pdf("Missing", b"");
    let mut model = read(&file);
    assert!(matches!(redacted(&mut model), Err(Refused::Invalid(_))));
    assert_eq!(saved(&model, &file), file);
}

#[test]
fn page_work_gives_views_a_new_file_to_load() {
    let file = pdf_file();
    let mut model = read(&file);
    let mut assets = Assets::default();
    let change = Change::Work {
        work: PageWork::Rotate {
            page: model.pages[0].id.clone(),
            degrees: 180,
        },
    };
    let done = Pdf::apply(&mut model, change, &[], &mut assets).unwrap();
    let Change::Worked { asset, pages, .. } = done else {
        panic!("not worked");
    };
    assert_eq!(asset, model.asset);
    assert_eq!(pages[0].rotation, 180);
    let hash: Vec<u8> = (0..64)
        .step_by(2)
        .map(|index| u8::from_str_radix(&asset[index..index + 2], 16).unwrap())
        .collect();
    assert!(assets.get(&<[u8; 32]>::try_from(hash).unwrap()).is_some());
}

#[test]
fn an_old_change_to_the_same_field_is_refused_with_the_name() {
    let mut model = read(&pdf_file());
    let earlier = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: Change::Field {
            name: "name".to_owned(),
            value: Some("x".to_owned()),
        },
    };
    let late = Change::Ops {
        ops: vec![Op::SetField {
            name: "name".to_owned(),
            value: Some("y".to_owned()),
        }],
    };
    match Pdf::apply(&mut model, late, &[earlier], &mut Assets::default()) {
        Err(Refused::Behind(detail)) => assert!(detail.contains("Ana")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_compare_finds_a_note_that_a_write_added() {
    let file = pdf_file();
    let base = read(&file);
    let mut other = read(&file);
    ops(
        &mut other,
        vec![Op::AddNote {
            page: 1,
            text: "Outside".to_owned(),
            x: 1.0,
            y: 1.0,
            width: None,
            height: None,
            kind: None,
        }],
    )
    .unwrap();
    let written = saved(&other, &file);
    let changes = Pdf::compare(&base, &read(&written));
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], Change::Note { .. }));
}

#[test]
fn a_read_gives_the_text_of_a_page() {
    let answer = Pdf::query(&read(&pdf_file()), Query::Read { page: Some(2) }).0;
    assert!(
        answer["pages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("second page")
    );
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = Pdf::guide();
    for op in &guide.ops {
        let mut example = json!({ "op": op.name });
        for field in op.fields.iter().filter(|field| field.required) {
            example[&field.name] = match field.kind.as_str() {
                "page number" | "number" | "points" => json!(1),
                "rectangles" => json!([[0.0, 0.0, 1.0, 1.0]]),
                _ => json!("x"),
            };
        }
        let parsed: Result<Op, _> = serde_json::from_value(example);
        assert!(parsed.is_ok(), "{}: {parsed:?}", op.name);
    }
}
