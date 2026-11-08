use document::{
    Assets, CONTRACT, Document, Format, Input, Ordered, Refused, ToDocument, Viewer, contract,
};
use serde_json::{Value, json};

use crate::{Change, Model, Plain, Query, diff, lines::Text};

fn read(file: &str) -> Model {
    Plain::read(file.as_bytes(), &mut Assets::default()).unwrap()
}

fn lines(model: &Model) -> Vec<String> {
    Text::new(&model.doc)
        .lines()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn apply(model: &mut Model, change: Change) -> Result<Change, Refused> {
    Plain::apply(model, change, &[], &mut Assets::default())
}

fn ops(model: &mut Model, ops: Value) -> Result<Change, Refused> {
    apply(
        model,
        Change::Ops {
            ops: serde_json::from_value(ops).unwrap(),
        },
    )
}

fn saved(model: &Model, file: &str) -> String {
    String::from_utf8(Plain::write(model, file.as_bytes(), &Assets::default()).unwrap()).unwrap()
}

fn merged(model: &mut Model, base: &str, written: &str) {
    for change in Plain::compare(&read(base), &read(written)) {
        drop(apply(model, change));
    }
}

#[test]
fn a_file_keeps_its_bytes_through_a_read_and_a_write() {
    for file in [
        "a\nb\n",
        "a\r\nb\r\n",
        "\u{feff}name: x\r\nlist:\r\n  - one",
        "a\r\nb\nc\n",
        "{\n  \"κλειδί\": \"日本\"\n}",
        "",
        "\u{feff}",
        "one\n\n\n",
    ] {
        assert_eq!(saved(&read(file), file), file);
    }
}

#[test]
fn a_save_keeps_the_mark_the_line_endings_and_the_last_line_ending() {
    let file = "\u{feff}a: 1\r\nb: 2";
    let mut model = read(file);
    ops(
        &mut model,
        json!([{ "op": "insertLines", "text": "c: 3\nd: 4\n" }]),
    )
    .unwrap();
    assert_eq!(saved(&model, file), "\u{feff}a: 1\r\nb: 2\r\nc: 3\r\nd: 4");
}

#[test]
fn an_empty_file_ends_with_a_line_ending_once_it_has_text_and_a_cleared_one_is_empty() {
    let mut model = read("");
    ops(
        &mut model,
        json!([{ "op": "insertLines", "text": "a: 1\nb: 2" }]),
    )
    .unwrap();
    assert_eq!(saved(&model, ""), "a: 1\nb: 2\n");
    ops(
        &mut model,
        json!([{ "op": "deleteLines", "from": 1, "to": 2 }]),
    )
    .unwrap();
    assert_eq!(lines(&model), [""]);
    assert_eq!(saved(&model, "a: 1\nb: 2\n"), "");
}

#[test]
fn a_long_file_does_not_open_live_but_stays_live_when_it_grows() {
    let most = "x\n".repeat(10_000);
    assert!(Plain::opens(most.as_bytes()).is_ok());
    let longer = format!("{most}x\n");
    assert!(Plain::opens(longer.as_bytes()).is_err());
    assert!(Plain::opens("x".repeat(600 * 1024).as_bytes()).is_err());
    let mut document = Document::<Plain>::open(most.into_bytes(), 1, Assets::default()).unwrap();
    document.step(Input::FileChanged {
        file: longer.into_bytes(),
        writer: "script".to_owned(),
    });
    assert_eq!(lines(document.model()).len(), 10_001);
}

#[test]
fn a_file_that_is_not_utf8_neither_opens_live_nor_reads() {
    assert!(Plain::opens(b"caf\xe9\n").is_err());
    assert!(Plain::read(b"caf\xe9\n", &mut Assets::default()).is_err());
    assert!(Plain::opens("\u{feff}café\n".as_bytes()).is_ok());
}

#[test]
fn replace_text_needs_one_place_unless_all_is_set_and_may_span_lines() {
    let mut model = read("on: true\nport: 80\nlog: true\n");
    let twice = ops(
        &mut model,
        json!([{ "op": "replaceText", "find": "true", "with": "no" }]),
    );
    assert!(matches!(twice, Err(Refused::Invalid(reason)) if reason.contains("2 places")));
    ops(
        &mut model,
        json!([{ "op": "replaceText", "find": "true", "with": "false", "all": true }]),
    )
    .unwrap();
    ops(
        &mut model,
        json!([{ "op": "replaceText", "find": "80\nlog", "with": "8080\r\nhost: x\nlog" }]),
    )
    .unwrap();
    assert_eq!(
        lines(&model),
        ["on: false", "port: 8080", "host: x", "log: false"]
    );
    let missing = ops(
        &mut model,
        json!([{ "op": "replaceText", "find": "none", "with": "x" }]),
    );
    assert!(matches!(missing, Err(Refused::Invalid(_))));
}

#[test]
fn the_line_numbers_of_one_change_count_in_the_text_as_it_was_read() {
    let mut model = read("1\n2\n3\n4\n5\n6\n");
    ops(
        &mut model,
        json!([
            { "op": "deleteLines", "from": 1 },
            { "op": "replaceLines", "from": 3, "to": 4, "text": "three and four" },
            { "op": "insertLines", "text": "five and a half", "after": 5 },
            { "op": "insertLines", "text": "zero", "after": 0 },
            { "op": "insertLines", "text": "seven" },
        ]),
    )
    .unwrap();
    assert_eq!(
        lines(&model),
        [
            "zero",
            "2",
            "three and four",
            "5",
            "five and a half",
            "6",
            "seven"
        ]
    );
    ops(
        &mut model,
        json!([{ "op": "deleteLines", "from": 6, "to": 7 }]),
    )
    .unwrap();
    assert_eq!(
        lines(&model).last().map(String::as_str),
        Some("five and a half")
    );
}

#[test]
fn ops_that_change_the_same_line_or_a_line_past_the_end_are_refused() {
    let mut model = read("a\nb\nc\n");
    let both = ops(
        &mut model,
        json!([
            { "op": "replaceText", "find": "b", "with": "x" },
            { "op": "deleteLines", "from": 2 },
        ]),
    );
    assert!(matches!(both, Err(Refused::Invalid(reason)) if reason.contains("line 2")));
    let past = ops(
        &mut model,
        json!([{ "op": "deleteLines", "from": 3, "to": 4 }]),
    );
    assert!(matches!(past, Err(Refused::Invalid(reason)) if reason.contains("3 lines")));
    let unknown: Result<Vec<crate::Op>, _> =
        serde_json::from_value(json!([{ "op": "insertLines", "text": "x", "line": 2 }]));
    assert!(unknown.is_err());
    assert_eq!(lines(&model), ["a", "b", "c"]);
}

#[test]
fn line_numbers_read_before_another_change_are_refused_and_found_text_is_not() {
    let mut model = read("a\nb\n");
    let earlier = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "view".to_owned(),
        seq: 1,
        change: Change::Steps { steps: Vec::new() },
    };
    let since = [earlier];
    let mut sent = |ops: Value| {
        Plain::apply(
            &mut model,
            Change::Ops {
                ops: serde_json::from_value(ops).unwrap(),
            },
            &since,
            &mut Assets::default(),
        )
    };
    let numbered = sent(json!([{ "op": "insertLines", "text": "x", "after": 1 }]));
    assert!(matches!(numbered, Err(Refused::Behind(reason)) if reason.starts_with("Ana")));
    assert!(sent(json!([{ "op": "replaceText", "find": "b", "with": "c" }])).is_ok());
    assert!(sent(json!([{ "op": "insertLines", "text": "d" }])).is_ok());
    assert_eq!(lines(&model), ["a", "c", "d"]);
}

#[test]
fn an_outside_write_keeps_what_a_person_changed_elsewhere() {
    let base = "one\ntwo\nthree\nfour\nfive\nsix\n";
    let mut model = read(base);
    ops(
        &mut model,
        json!([{ "op": "replaceText", "find": "five", "with": "FIVE" }]),
    )
    .unwrap();
    merged(
        &mut model,
        base,
        "zero\none\n2\nthree\nfour\nfive\nsix\nseven\n",
    );
    assert_eq!(
        lines(&model),
        ["zero", "one", "2", "three", "four", "FIVE", "six", "seven"]
    );
}

#[test]
fn where_a_person_and_a_writer_changed_the_same_line_the_writer_wins_and_only_there() {
    let base = "a\nb\nc\nd\ne\n";
    let mut model = read(base);
    ops(
        &mut model,
        json!([{ "op": "replaceLines", "from": 2, "to": 3, "text": "B\nC" }]),
    )
    .unwrap();
    merged(&mut model, base, "a\nb\nwriter\nwriter\ne\n");
    assert_eq!(lines(&model), ["a", "B", "writer", "writer", "e"]);
}

#[test]
fn a_write_that_the_text_has_already_changes_nothing() {
    let base = "a\nb\n";
    let mut model = read(base);
    ops(&mut model, json!([{ "op": "insertLines", "text": "c" }])).unwrap();
    let changes = Plain::compare(&read(base), &read("a\nb\nc\n"));
    let [change] = changes.as_slice() else {
        panic!("{changes:?}");
    };
    assert!(apply(&mut model, change.clone()).is_err());
    assert_eq!(lines(&model), ["a", "b", "c"]);
}

fn viewer(name: &str) -> Viewer {
    Viewer {
        id: name.to_owned(),
        name: name.to_owned(),
        can_edit: true,
    }
}

fn message(document: &mut Document<Plain>, message: &ToDocument) {
    document.step(Input::Message {
        pipe: 1,
        frame: contract::encode_message(message),
    });
}

#[test]
fn an_outside_write_merges_from_the_saved_state_that_its_writer_read() {
    let original = "title\nb\nc\nd\n";
    let mut document =
        Document::<Plain>::open(original.as_bytes().to_vec(), 1, Assets::default()).unwrap();
    document.step(Input::Connected {
        pipe: 1,
        viewer: viewer("ana"),
    });
    message(
        &mut document,
        &ToDocument::Join {
            contract: CONTRACT,
            client: "ana".to_owned(),
            epoch: 0,
            rev: 0,
        },
    );
    let mut retitled = |seq, base, title: &str| {
        message(
            &mut document,
            &ToDocument::Change {
                seq,
                base,
                body: json!({
                    "kind": "ops",
                    "ops": [{ "op": "replaceLines", "from": 1, "text": title }],
                }),
            },
        );
    };
    retitled(1, 0, "saved title");
    retitled(2, 1, "newest title");
    document.step(Input::Saved { rev: 1 });
    document.step(Input::FileChanged {
        file: b"saved title\nb\nc\nd\nwritten\n".to_vec(),
        writer: "script".to_owned(),
    });
    assert_eq!(
        lines(document.model()),
        ["newest title", "b", "c", "d", "written"]
    );
}

#[test]
fn a_read_numbers_the_lines_and_cuts_long_ones() {
    let long = "x".repeat(2100);
    let model = read(&format!("a\n{long}\nc\n"));
    let answer = Plain::query(
        &model,
        Query::Read {
            from: Some(2),
            to: Some(9),
        },
    )
    .0;
    assert_eq!(answer["count"], 3);
    assert_eq!(answer["lines"][1], json!({ "line": 3, "text": "c" }));
    let cut = answer["lines"][0]["text"].as_str().unwrap();
    assert_eq!(cut.chars().count(), 2001);
    assert!(cut.ends_with('…'));
}

#[test]
fn a_place_reads_as_lines_a_column_and_the_selected_text() {
    let model = read("first\nsecond\nthird\n");
    let place = Plain::describe_place(&model, &json!({ "from": 10, "to": 18 }));
    assert_eq!(
        place,
        json!({ "lines": "2-3", "column": 3, "selected": "cond\nth" })
    );
    let caret = Plain::describe_place(&model, &json!({ "from": 1, "to": 1 }));
    assert_eq!(caret, json!({ "lines": "1", "column": 1 }));
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = Plain::guide();
    for op in &guide.ops {
        let mut example = json!({ "op": op.name });
        for field in op.fields.iter().filter(|field| field.required) {
            example[&field.name] = match field.kind.as_str() {
                "line number" => json!(1),
                _ => json!("x"),
            };
        }
        let parsed: Result<crate::Op, _> = serde_json::from_value(example);
        assert!(parsed.is_ok(), "{}: {parsed:?}", op.name);
    }
}

/// The length of the longest common subsequence, the slow way.
fn common(old: &[u8], new: &[u8]) -> usize {
    let mut table = vec![vec![0; new.len() + 1]; old.len() + 1];
    for (row, one) in old.iter().enumerate() {
        for (column, other) in new.iter().enumerate() {
            table[row + 1][column + 1] = if one == other {
                table[row][column] + 1
            } else {
                table[row][column + 1].max(table[row + 1][column])
            };
        }
    }
    table[old.len()][new.len()]
}

#[test]
fn a_diff_keeps_as_many_lines_as_the_texts_share_and_its_stretches_make_the_new_text() {
    let mut seed = 7_u64;
    let mut next = |below: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        u8::try_from((seed >> 33) % below).unwrap()
    };
    for _ in 0..200 {
        let old: Vec<u8> = (0..next(30)).map(|_| next(4)).collect();
        let new: Vec<u8> = (0..next(30)).map(|_| next(4)).collect();
        let kept = diff::kept(&old, &new);
        assert_eq!(kept.len(), common(&old, &new), "{old:?} {new:?}");
        assert!(kept.iter().all(|(at, to)| old[*at] == new[*to]));
        assert!(
            kept.windows(2)
                .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1)
        );
        let mut rebuilt: Vec<u8> = Vec::new();
        let mut at = 0;
        for (from, to) in diff::changed(&old, &new) {
            rebuilt.extend(&old[at..from.start]);
            rebuilt.extend(&new[to]);
            at = from.end;
        }
        rebuilt.extend(&old[at..]);
        assert_eq!(rebuilt, new);
    }
}
