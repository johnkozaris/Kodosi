use std::path::Path;

use base64::Engine as _;
use serde_json::json;

use super::{
    BrowserAct, BrowserDoingKind, BrowserPoint, CanvasAnswer, CanvasHandle, CanvasKind,
    CanvasRequest, MOST_ITEMS, PART_BYTES, browser, files::Files, files::Stamp, kind_of, page,
};

#[test]
fn a_file_shows_by_its_extension() {
    assert_eq!(kind_of(Path::new("/a/plan.MD")), Some(CanvasKind::Page));
    assert_eq!(kind_of(Path::new("/a/shot.png")), Some(CanvasKind::Picture));
    assert_eq!(kind_of(Path::new("/a/paper.pdf")), Some(CanvasKind::Pdf));
    assert_eq!(
        kind_of(Path::new("/a/notes.pages")),
        Some(CanvasKind::Document)
    );
    assert_eq!(kind_of(Path::new("/a/plan.xlsx")), Some(CanvasKind::Sheet));
    assert_eq!(
        kind_of(Path::new("/a/talk.KEY")),
        Some(CanvasKind::Presentation)
    );
    assert_eq!(kind_of(Path::new("/a/run.sh")), None);
    assert_eq!(kind_of(Path::new("/a/README")), None);
}

#[test]
fn requests_from_other_computers_have_bounds() {
    let read = |key: String| CanvasRequest::Read {
        key,
        revision: 1,
        offset: 0,
    };
    assert!(read("/a/plan.md".to_owned()).is_valid());
    assert!(!read("a".repeat(2000)).is_valid());
    assert!(!read("/a/\0".to_owned()).is_valid());
}

#[test]
fn a_request_and_an_answer_have_their_wire_form() {
    let request: CanvasRequest = serde_json::from_value(json!({
        "type": "act",
        "action": {"do": "wake"},
    }))
    .unwrap();
    assert!(matches!(
        request,
        CanvasRequest::Act {
            action: BrowserAct::Wake
        }
    ));
    assert!(request.is_valid());
    assert!(
        serde_json::from_value::<CanvasRequest>(json!({
            "type": "act",
            "action": {"do": "open", "url": "http://localhost:3000"},
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<CanvasRequest>(json!({"type": "remove", "keys": [], "x": 1}))
            .is_err()
    );
    let answer = CanvasAnswer::Stale {
        key: "/a/plan.md".into(),
        revision: 3,
    };
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({"answer": "stale", "key": "/a/plan.md", "revision": 3})
    );
}

#[tokio::test]
async fn the_newest_file_comes_first_and_a_change_makes_a_new_revision() {
    let folder = tempfile::tempdir().unwrap();
    let folder = kodosi_os::fs::canonicalize(folder.path()).unwrap();
    let plan = folder.join("plan.md");
    let shot = folder.join("shot.png");
    std::fs::write(&plan, "# Plan").unwrap();
    std::fs::write(&shot, [1, 2, 3]).unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    files.show(vec![plan.clone()], &stamp).await.unwrap();
    files.show(vec![shot.clone()], &stamp).await.unwrap();
    let names = |files: &Files| {
        files
            .items()
            .into_iter()
            .map(|item| item.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&files), ["shot.png", "plan.md"]);
    files.show(vec![plan.clone()], &stamp).await.unwrap();
    assert_eq!(names(&files), ["plan.md", "shot.png"]);
    assert_eq!(files.items()[0].revision, 1);

    std::fs::write(&plan, "# Plan\n\nMore words.").unwrap();
    assert!(files.check(&stamp).await);
    assert_eq!(files.items()[0].revision, 2);
    assert!(!files.check(&stamp).await);

    std::fs::remove_file(&shot).unwrap();
    assert!(files.check(&stamp).await);
    assert!(files.items()[1].missing);
    let key = files.items()[1].key.clone();
    assert!(files.read(&key, 1, 0).await.is_err());
}

#[tokio::test]
async fn the_canvas_refuses_what_it_cannot_show() {
    let folder = tempfile::tempdir().unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    let script = folder.path().join("run.sh");
    std::fs::write(&script, "echo").unwrap();
    assert!(files.show(vec![script], &stamp).await.is_err());
    assert!(
        files
            .show(vec![folder.path().join("gone.md")], &stamp)
            .await
            .is_err()
    );
    assert!(files.is_empty());
}

#[tokio::test]
async fn a_person_adds_a_file_in_parts_and_a_taken_name_gets_a_number() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path()),
    );
    canvas.moved_to(folder.path().to_owned());
    let attach = |upload: uuid::Uuid, offset, data: &[u8], last| CanvasRequest::Attach {
        upload: upload.to_string(),
        name: "notes.md".to_owned(),
        offset,
        data: base64::engine::general_purpose::STANDARD.encode(data),
        last,
        page: false,
    };
    for number in ["notes.md", "notes 2.md"] {
        let upload = uuid::Uuid::now_v7();
        let first = canvas.request(attach(upload, 0, b"# He", false)).await;
        assert_eq!(first.unwrap(), CanvasAnswer::Done);
        let path = folder.path().join(number);
        assert!(!path.exists());
        let last = canvas.request(attach(upload, 4, b"llo", true)).await;
        let CanvasAnswer::Attached { key } = last.unwrap() else {
            panic!("attached");
        };
        assert!(key.ends_with(number), "{key}");
        assert_eq!(std::fs::read(&path).unwrap(), b"# Hello");
    }
    let items = canvas.current().unwrap().items;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "notes 2.md");
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn a_lost_part_or_a_file_of_no_kind_leaves_nothing_behind() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path()),
    );
    canvas.moved_to(folder.path().to_owned());
    let attach = |name: &str, upload: uuid::Uuid, offset| CanvasRequest::Attach {
        upload: upload.to_string(),
        name: name.to_owned(),
        offset,
        data: "AAAA".to_owned(),
        last: false,
        page: false,
    };
    let upload = uuid::Uuid::now_v7();
    canvas.request(attach("a.md", upload, 0)).await.unwrap();
    let lost = canvas.request(attach("a.md", upload, 9)).await.unwrap_err();
    assert!(lost.contains("got lost"), "{lost}");
    let kind = canvas
        .request(attach("a.zip", uuid::Uuid::now_v7(), 0))
        .await
        .unwrap_err();
    assert!(kind.contains("The canvas shows"), "{kind}");
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn a_file_for_a_page_of_any_kind_goes_into_the_folder_and_not_on_the_canvas() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path()),
    );
    canvas.moved_to(folder.path().to_owned());
    let attached = canvas
        .request(CanvasRequest::Attach {
            upload: uuid::Uuid::now_v7().to_string(),
            name: "data.zip".to_owned(),
            offset: 0,
            data: "UEsDBA==".to_owned(),
            last: true,
            page: true,
        })
        .await
        .unwrap();
    let CanvasAnswer::Attached { key } = attached else {
        panic!("attached");
    };
    assert_eq!(std::fs::read(&key).unwrap(), b"PK\x03\x04");
    assert!(
        canvas
            .current()
            .is_none_or(|canvas| canvas.items.is_empty())
    );
}

#[test]
fn a_file_name_has_no_folders() {
    let attach = |name: &str| CanvasRequest::Attach {
        upload: uuid::Uuid::now_v7().to_string(),
        name: name.to_owned(),
        offset: 0,
        data: String::new(),
        last: true,
        page: false,
    };
    assert!(attach("plan.docx").is_valid());
    for name in ["", "..", "a/b.md", "a\\b.md", "C:b.md"] {
        assert!(!attach(name).is_valid(), "{name}");
    }
}

#[tokio::test]
async fn the_canvas_keeps_the_newest_items() {
    let folder = tempfile::tempdir().unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    for number in 0..=MOST_ITEMS {
        let path = folder.path().join(format!("{number}.txt"));
        std::fs::write(&path, "x").unwrap();
        let showing = files.show(vec![path], &stamp).await.unwrap();
        assert_eq!(showing.dropped.len(), usize::from(number == MOST_ITEMS));
    }
    assert_eq!(files.items().len(), MOST_ITEMS);
    assert_eq!(files.items()[0].name, format!("{MOST_ITEMS}.txt"));
    let key = files.items()[0].key.clone();
    assert_eq!(files.remove(&[key]), 1);
    assert_eq!(files.remove(&[]), MOST_ITEMS - 1);
    assert!(files.is_empty());
}

#[tokio::test]
async fn a_large_file_comes_in_parts_of_one_revision() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("big.pdf");
    let content = (0..=u8::MAX)
        .cycle()
        .take(PART_BYTES + 10)
        .collect::<Vec<_>>();
    std::fs::write(&path, &content).unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    files.show(vec![path.clone()], &stamp).await.unwrap();
    let key = files.items()[0].key.clone();
    let mut read = Vec::new();
    let mut offset = 0;
    loop {
        let CanvasAnswer::Part {
            total, data, kind, ..
        } = files.read(&key, 1, offset).await.unwrap()
        else {
            panic!("expected a part");
        };
        assert_eq!(kind, CanvasKind::Pdf);
        read.extend(
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap(),
        );
        offset = read.len() as u64;
        if offset == total {
            break;
        }
    }
    assert_eq!(read, content);
    assert!(files.read(&key, 1, offset).await.is_err());
    assert_eq!(
        files.read(&key, 7, 0).await.unwrap(),
        CanvasAnswer::Stale { key, revision: 1 }
    );
}

#[test]
fn markdown_and_text_become_pages_with_the_template_look() {
    let markdown = String::from_utf8(page::page(
        "plan.md",
        b"# Plan\n\n| a | b |\n|---|---|\n| 1 | 2 |",
    ))
    .unwrap();
    assert!(markdown.contains("<h1>Plan</h1>"));
    assert!(markdown.contains("<table>"));
    assert!(markdown.contains("--accent"));
    assert!(markdown.contains("<title>plan.md</title>"));
    let text = String::from_utf8(page::page("run.log", b"<b>&</b>")).unwrap();
    assert!(text.contains("<pre><code>&lt;b&gt;&amp;&lt;/b&gt;</code></pre>"));
}

#[test]
fn a_page_that_loads_from_the_network_is_named() {
    assert_eq!(
        page::network_reference(br#"<script src="https://cdn.example/x.js"></script>"#),
        Some("https://cdn.example/x.js".to_owned())
    );
    assert_eq!(
        page::network_reference(br#"<link rel="stylesheet" href="https://fonts.example/a.css">"#),
        Some("https://fonts.example/a.css".to_owned())
    );
    assert_eq!(
        page::network_reference(br#"<a href="https://example.com">a link</a><style>p{}</style>"#),
        None
    );
}

#[test]
fn the_browser_keeps_its_session_and_hides_what_an_agent_types() {
    let refused = |arguments: &[&str]| {
        browser::refused(
            &arguments
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
        .map(str::to_owned)
    };
    for arguments in [
        &["--session", "x", "open"][..],
        &["open", "--profile=/x"],
        &["connect", "9222"],
        &["--cdp", "9222", "snapshot"],
        &["install"],
        &["close", "--all"],
    ] {
        assert!(refused(arguments).is_some(), "{arguments:?}");
    }
    for arguments in [
        &["open", "https://example.com"][..],
        &["snapshot", "-i", "--json"],
        &["fill", "@e3", "--all"],
        &["skills", "get", "--all"],
        &["close"],
    ] {
        assert_eq!(refused(arguments), None, "{arguments:?}");
    }
    let words = |arguments: &[&str]| {
        browser::words(
            &arguments
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        words(&["open", "https://example.com"]),
        "open https://example.com"
    );
    assert_eq!(words(&["fill", "@e3", "secret"]), "fill");
    assert_eq!(words(&["click", "@e2", "--new-tab"]), "click @e2");
}

#[test]
fn people_see_what_the_agent_does_and_where() {
    let doing = |arguments: &[&str]| {
        browser::doing(
            &arguments
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
    };
    let at = |target: &str| Some(target.to_owned());
    assert_eq!(
        doing(&["open", "https://example.com"]),
        Some((BrowserDoingKind::Open, None))
    );
    assert_eq!(
        doing(&["click", "@e2", "--new-tab"]),
        Some((BrowserDoingKind::Click, at("@e2")))
    );
    assert_eq!(
        doing(&["fill", "#email", "ana@example.com"]),
        Some((BrowserDoingKind::Type, at("#email")))
    );
    assert_eq!(
        doing(&["press", "Enter"]),
        Some((BrowserDoingKind::Type, None))
    );
    assert_eq!(
        doing(&["snapshot", "-i"]),
        Some((BrowserDoingKind::Read, None))
    );
    assert_eq!(
        doing(&["find", "role", "button", "click", "--name", "Send"]),
        Some((BrowserDoingKind::Click, None))
    );
    assert_eq!(doing(&["tab"]), Some((BrowserDoingKind::Read, None)));
    assert_eq!(doing(&["tab", "new"]), Some((BrowserDoingKind::Open, None)));
    assert_eq!(doing(&["session", "info"]), None);

    let box_at =
        r#"{"success":true,"data":{"height":20,"width":200,"x":100.5,"y":40},"error":null}"#;
    assert_eq!(
        browser::middle(box_at, 1280, 720),
        Some(BrowserPoint { x: 200, y: 50 })
    );
    assert_eq!(browser::middle(box_at, 1280, 45), None);
    assert_eq!(
        browser::middle(
            r#"{"success":true,"data":{"height":10,"width":100,"x":-300,"y":10},"error":null}"#,
            1280,
            720
        ),
        None
    );
    assert_eq!(
        browser::middle(
            r#"{"success":false,"data":null,"error":"Unknown ref: e9"}"#,
            1280,
            720
        ),
        None
    );
}
