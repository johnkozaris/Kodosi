use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::json;

use super::{
    BROWSER_PIPE, BrowserAct, BrowserDoingKind, BrowserPoint, BrowserState, CanvasAnswer,
    CanvasHandle, CanvasKind, CanvasRequest, DocumentView, MOST_ITEMS, PART_BYTES,
    browser::{self, Browser, Places, Terminal},
    engine::BrowserHost,
    files::{Files, QUIET, Showing, Stamp, examine_all},
    is_address, kind_of, page,
};
use crate::text::Text;

async fn show(files: &mut Files, paths: Vec<PathBuf>, stamp: &Stamp) -> Result<Showing, Text> {
    Ok(files.show(examine_all(paths).await?, stamp))
}

#[test]
fn show_takes_web_and_file_addresses_for_the_browser() {
    assert!(is_address("http://localhost:5173"));
    assert!(is_address("HTTPS://example.com/a?b=c"));
    assert!(is_address("file:///tmp/report.html"));
    assert!(!is_address("report.html"));
    assert!(!is_address("localhost:3000"));
    assert!(!is_address("ftp://example.com"));
    assert!(!is_address("https://example.com/\nmore"));
}

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
    show(&mut files, vec![plan.clone()], &stamp).await.unwrap();
    let first = files.items()[0].revision;
    show(&mut files, vec![shot.clone()], &stamp).await.unwrap();
    let names = |files: &Files| {
        files
            .items()
            .into_iter()
            .map(|item| item.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&files), ["shot.png", "plan.md"]);
    show(&mut files, vec![plan.clone()], &stamp).await.unwrap();
    assert_eq!(names(&files), ["plan.md", "shot.png"]);
    assert_eq!(files.items()[0].revision, first);

    std::fs::write(&plan, "# Plan\n\nMore words.").unwrap();
    assert!(files.check(&stamp).await.any);
    assert!(files.items()[0].revision > first);
    assert!(!files.check(&stamp).await.any);

    std::fs::remove_file(&shot).unwrap();
    assert!(files.check(&stamp).await.any);
    assert!(files.items()[1].missing);
    let key = files.items()[1].key.clone();
    assert!(files.read(&key, 1, 0).await.is_err());
}

#[tokio::test]
async fn a_text_file_opens_live_while_its_format_takes_it_and_else_shows_as_a_page() {
    let folder = tempfile::tempdir().unwrap();
    let folder = kodosi_os::fs::canonicalize(folder.path()).unwrap();
    let config = folder.join("config.yaml");
    let notes = folder.join("notes.txt");
    std::fs::write(&config, "port: 80\n").unwrap();
    std::fs::write(&notes, b"caf\xe9\n").unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    show(&mut files, vec![notes, config.clone()], &stamp)
        .await
        .unwrap();
    let views = |files: &Files| {
        files
            .items()
            .into_iter()
            .map(|item| item.view)
            .collect::<Vec<_>>()
    };
    assert_eq!(views(&files), [Some(DocumentView::Text), None]);

    std::fs::write(&config, "port: 80\n".repeat(20_000)).unwrap();
    assert!(files.check(&stamp).await.any);
    assert_eq!(views(&files), [None, None]);
}

#[tokio::test]
async fn the_canvas_refuses_what_it_cannot_show() {
    let folder = tempfile::tempdir().unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    let script = folder.path().join("run.sh");
    std::fs::write(&script, "echo").unwrap();
    assert!(show(&mut files, vec![script], &stamp).await.is_err());
    assert!(
        show(&mut files, vec![folder.path().join("gone.md")], &stamp)
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
        crate::canvas::CanvasServices::new(folder.path(), true),
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
        crate::canvas::CanvasServices::new(folder.path(), true),
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
    assert!(lost.to_string().contains("got lost"), "{lost}");
    let kind = canvas
        .request(attach("a.zip", uuid::Uuid::now_v7(), 0))
        .await
        .unwrap_err();
    assert!(kind.to_string().contains("The canvas shows"), "{kind}");
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn a_file_for_a_page_of_any_kind_goes_into_the_folder_and_not_on_the_canvas() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path(), true),
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
async fn showing_opens_the_canvas_at_the_first_file_until_that_file_goes() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path(), true),
    );
    canvas.moved_to(folder.path().to_owned());
    let plan = folder.path().join("plan.md");
    let shot = folder.path().join("shot.png");
    std::fs::write(&plan, "# Plan").unwrap();
    std::fs::write(&shot, [1, 2, 3]).unwrap();
    let showing = |paths: Vec<PathBuf>| crate::canvas::Show {
        paths,
        address: None,
        folder: folder.path().to_owned(),
        terminal: "Ivory Moose".to_owned(),
        agent: None,
    };

    let answer = canvas
        .show(showing(vec![plan, shot.clone()]))
        .await
        .unwrap();
    let CanvasAnswer::Text { text, code: 0 } = answer else {
        panic!("shown");
    };
    assert!(text.contains("opens for the people"), "{text}");
    let first = canvas.current().unwrap().open.unwrap();
    assert!(first.key.ends_with("plan.md"), "{}", first.key);

    let added = canvas
        .request(CanvasRequest::Attach {
            upload: uuid::Uuid::now_v7().to_string(),
            name: "notes.md".to_owned(),
            offset: 0,
            data: "IyBOb3Rlcw==".to_owned(),
            last: true,
            page: false,
        })
        .await;
    assert!(matches!(added, Ok(CanvasAnswer::Attached { .. })));
    assert_eq!(canvas.current().unwrap().open, Some(first.clone()));

    canvas.show(showing(vec![shot])).await.unwrap();
    let again = canvas.current().unwrap().open.unwrap();
    assert!(again.key.ends_with("shot.png"), "{}", again.key);
    assert!(again.at > first.at);

    canvas
        .request(CanvasRequest::Remove {
            keys: vec![again.key],
        })
        .await
        .unwrap();
    assert_eq!(canvas.current().unwrap().open, None);
}

#[tokio::test]
async fn a_browser_that_came_back_asleep_keeps_its_page_for_the_next_start() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path(), true),
    );
    let page = "https://example.com/report";
    canvas.restore(Vec::new(), Some(page.to_owned()), uuid::Uuid::now_v7());
    canvas
        .state()
        .wait_for(|canvas| canvas.browser.is_some())
        .await
        .unwrap();
    assert_eq!(canvas.address().as_deref(), Some(page));

    canvas
        .request(CanvasRequest::Remove {
            keys: vec![BROWSER_PIPE.to_owned()],
        })
        .await
        .unwrap();
    assert_eq!(canvas.address(), None);
}

#[tokio::test(start_paused = true)]
async fn the_agent_browser_opens_the_canvas_at_it_after_a_quiet_minute() {
    let folder = tempfile::tempdir().unwrap();
    let services = crate::canvas::CanvasServices {
        agent_browser: folder.path().join("agent-browser"),
        ..crate::canvas::CanvasServices::new(folder.path(), true)
    };
    std::fs::write(&services.agent_browser, "").unwrap();
    let spawn = || CanvasHandle::spawn(uuid::Uuid::now_v7(), folder.path(), services.clone());
    // Kodosi refuses this command, so no agent-browser runs.
    let browse = async |canvas: &CanvasHandle| {
        canvas
            .browse(
                folder.path().to_owned(),
                vec!["close".to_owned(), "--all".to_owned()],
                "Ivory Moose".to_owned(),
                None,
            )
            .await
    };

    let canvas = spawn();
    assert!(browse(&canvas).await.is_err());
    let first = canvas.current().unwrap().open.unwrap();
    assert_eq!(first.key, BROWSER_PIPE);
    assert!(browse(&canvas).await.is_err());
    assert_eq!(canvas.current().unwrap().open, Some(first.clone()));

    tokio::time::advance(QUIET).await;
    assert!(browse(&canvas).await.is_err());
    let again = canvas.current().unwrap().open.unwrap();
    assert_eq!(again.key, BROWSER_PIPE);
    assert!(again.at > first.at);

    let restored = spawn();
    restored.restore(
        Vec::new(),
        Some("https://example.com/report".to_owned()),
        uuid::Uuid::now_v7(),
    );
    assert!(browse(&restored).await.is_err());
    let back = restored.current().unwrap();
    assert!(back.browser.is_some());
    assert_eq!(
        back.open.map(|open| open.key).as_deref(),
        Some(BROWSER_PIPE)
    );
}

#[tokio::test(start_paused = true)]
async fn a_file_that_changes_after_a_quiet_minute_opens_the_canvas_unless_it_is_live() {
    let folder = tempfile::tempdir().unwrap();
    let services = crate::canvas::CanvasServices::new(folder.path(), true);
    let canvas = CanvasHandle::spawn(uuid::Uuid::now_v7(), folder.path(), services.clone());
    let notes = folder.path().join("notes.txt");
    let report = folder.path().join("report.md");
    std::fs::write(&notes, "one\n").unwrap();
    std::fs::write(&report, "# One").unwrap();
    canvas
        .show(crate::canvas::Show {
            paths: vec![notes.clone(), report.clone()],
            address: None,
            folder: folder.path().to_owned(),
            terminal: "Ivory Moose".to_owned(),
            agent: None,
        })
        .await
        .unwrap();
    let shown = canvas.current().unwrap().open.unwrap();
    assert!(shown.key.ends_with("notes.txt"), "{}", shown.key);
    let changed = async |key: &str| {
        let revision = |canvas: &super::Canvas| {
            canvas
                .items
                .iter()
                .find(|item| item.key.ends_with(key))
                .map(|item| item.revision)
        };
        let before = revision(&canvas.current().unwrap());
        canvas
            .state()
            .wait_for(|canvas| revision(canvas) != before)
            .await
            .unwrap();
    };

    std::fs::write(&report, "# Two, soon after").unwrap();
    changed("report.md").await;
    assert_eq!(canvas.current().unwrap().open, Some(shown.clone()));

    tokio::time::advance(QUIET * 2).await;
    std::fs::write(&report, "# Three, after a quiet while").unwrap();
    changed("report.md").await;
    let opened = canvas.current().unwrap().open.unwrap();
    assert!(opened.key.ends_with("report.md"), "{}", opened.key);
    assert!(opened.at > shown.at);

    let _live = services
        .documents
        .open(
            &notes,
            documents::Viewer {
                id: "ana".to_owned(),
                name: "Ana".to_owned(),
                can_edit: true,
            },
        )
        .await
        .unwrap();
    tokio::time::advance(QUIET * 2).await;
    std::fs::write(&notes, "one\ntwo, from another program\n").unwrap();
    changed("notes.txt").await;
    assert_eq!(canvas.current().unwrap().open, Some(opened));
}

#[tokio::test(start_paused = true)]
async fn the_agent_edit_of_a_document_opens_the_canvas_at_it_after_a_quiet_minute() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path(), true),
    );
    let doc = async |words: &[&str]| {
        let arguments = words.iter().map(|word| (*word).to_owned()).collect();
        match canvas
            .document_command(
                None,
                "Ivory Moose".to_owned(),
                folder.path().to_owned(),
                arguments,
            )
            .await
        {
            CanvasAnswer::Text { text, code } => (text, code),
            other => panic!("text expected: {other:?}"),
        }
    };
    let apply = async || {
        let (read, _) = doc(&["read", "sums.xlsx"]).await;
        let rev = serde_json::from_str::<serde_json::Value>(&read).unwrap()["rev"].to_string();
        let ops = json!([{ "op": "setCell", "sheet": "Sheet1", "cell": "B2", "value": 7 }]);
        let (text, code) = doc(&[
            "apply",
            "sums.xlsx",
            "--ops",
            &ops.to_string(),
            "--base",
            &rev,
        ])
        .await;
        assert_eq!(code, 0, "{text}");
    };
    assert_eq!(doc(&["new", "sums.xlsx"]).await.1, 0);
    canvas
        .show(crate::canvas::Show {
            paths: vec![folder.path().join("sums.xlsx")],
            address: None,
            folder: folder.path().to_owned(),
            terminal: "Ivory Moose".to_owned(),
            agent: None,
        })
        .await
        .unwrap();
    let shown = canvas.current().unwrap().open.unwrap();

    apply().await;
    tokio::task::yield_now().await;
    assert_eq!(canvas.current().unwrap().open, Some(shown.clone()));

    tokio::time::advance(QUIET * 2).await;
    apply().await;
    let opened = canvas
        .state()
        .wait_for(|canvas| canvas.open.as_ref().is_some_and(|open| open.at > shown.at))
        .await
        .unwrap()
        .open
        .clone()
        .unwrap();
    assert_eq!(opened.key, shown.key);
}

#[tokio::test]
async fn the_canvas_keeps_the_newest_items() {
    let folder = tempfile::tempdir().unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    for number in 0..=MOST_ITEMS {
        let path = folder.path().join(format!("{number}.txt"));
        std::fs::write(&path, "x").unwrap();
        let showing = show(&mut files, vec![path], &stamp).await.unwrap();
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
async fn a_file_that_comes_back_gets_a_new_revision_with_its_new_content() {
    let folder = tempfile::tempdir().unwrap();
    let report = folder.path().join("report.md");
    std::fs::write(&report, "# Old report").unwrap();
    let stamp = Stamp::default();
    let mut files = Files::default();
    let newest = async |files: &mut Files| {
        let item = files.items()[0].clone();
        let CanvasAnswer::Part { data, .. } =
            files.read(&item.key, item.revision, 0).await.unwrap()
        else {
            panic!("expected a part");
        };
        let page = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        (item.revision, String::from_utf8(page).unwrap())
    };
    show(&mut files, vec![report.clone()], &stamp)
        .await
        .unwrap();
    let key = files.items()[0].key.clone();
    let (first, page) = newest(&mut files).await;
    assert!(page.contains("Old report"), "{page}");

    for number in 0..MOST_ITEMS {
        let other = folder.path().join(format!("{number}.txt"));
        std::fs::write(&other, "x").unwrap();
        show(&mut files, vec![other], &stamp).await.unwrap();
    }
    assert!(!files.contains(&key));
    std::fs::write(&report, "# New report").unwrap();
    show(&mut files, vec![report.clone()], &stamp)
        .await
        .unwrap();
    let (second, page) = newest(&mut files).await;
    assert!(second > first);
    assert!(page.contains("New report"), "{page}");

    assert_eq!(files.remove(std::slice::from_ref(&key)), 1);
    show(&mut files, vec![report], &stamp).await.unwrap();
    assert!(files.items()[0].revision > second);
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
    show(&mut files, vec![path.clone()], &stamp).await.unwrap();
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

#[tokio::test]
async fn a_browser_ends_at_once_while_kodosi_browser_comes() {
    let folder = tempfile::tempdir().unwrap();
    let host = BrowserHost::new(folder.path(), true);
    let Some(download) = host.hold_downloads().await else {
        return;
    };
    let browser = Browser::start(
        Terminal {
            id: uuid::Uuid::now_v7(),
            folder: tokio::sync::watch::channel(folder.path().to_owned()).1,
            shared: tokio::sync::watch::channel(false).1,
            show: Box::new(|_| {}),
        },
        Places::of(folder.path(), browser::agent_browser()),
        host,
        Stamp::default(),
    );
    let mut state = browser.state();
    let waking = tokio::spawn({
        let browser = std::sync::Arc::clone(&browser);
        async move { browser.act(BrowserAct::Wake).await }
    });
    state
        .wait_for(|state| state.state == BrowserState::Starting)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), browser.end())
        .await
        .expect("the browser ended while Kodosi Browser came");
    waking.abort();
    assert!(waking.await.unwrap_err().is_cancelled());
    drop(download);
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
