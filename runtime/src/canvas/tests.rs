use std::path::PathBuf;

use base64::Engine as _;

use super::{
    BrowserAct, BrowserState, Canvas, CanvasAnswer, CanvasHandle, CanvasKind, CanvasOpen,
    CanvasRequest, MOST_ITEMS, PART_BYTES,
    browser::{self, Browser, Places, Terminal},
    engine::BrowserHost,
    files::{Files, Showing, Stamp, examine_all},
};
use crate::text::Text;

async fn show(files: &mut Files, paths: Vec<PathBuf>, stamp: &Stamp) -> Result<Showing, Text> {
    Ok(files.show(examine_all(paths).await?, stamp))
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

    let mark = |key: &str| {
        Some(CanvasOpen {
            key: key.to_owned(),
            at: 1,
        })
    };
    let canvas = |open, change| Canvas {
        open,
        change,
        ..Canvas::default()
    };
    assert!(canvas(mark("/a/plan.md"), mark("/a/plan.md")).is_valid());
    assert!(!canvas(mark(&"a".repeat(2000)), None).is_valid());
    assert!(!canvas(None, mark("/a/\0")).is_valid());
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
async fn the_browser_of_a_terminal_that_becomes_shared_says_it_keeps_its_own_sign_ins() {
    let folder = tempfile::tempdir().unwrap();
    let canvas = CanvasHandle::spawn(
        uuid::Uuid::now_v7(),
        folder.path(),
        crate::canvas::CanvasServices::new(folder.path(), true),
    );
    canvas.restore(
        Vec::new(),
        Some("https://example.com".to_owned()),
        false,
        uuid::Uuid::now_v7(),
    );
    let mut state = canvas.state();
    let own = |canvas: &Canvas| canvas.browser.as_ref().map(|browser| browser.own_sign_ins);
    state
        .wait_for(|canvas| own(canvas) == Some(false))
        .await
        .unwrap();

    canvas.set_shared(true);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.wait_for(|canvas| own(canvas) == Some(true)),
    )
    .await
    .expect("the browser says that it keeps its own sign-ins")
    .unwrap();
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

#[cfg(unix)]
#[tokio::test]
async fn a_long_answer_of_agent_browser_is_cut_at_a_character_and_still_ends() {
    let mut command = tokio::process::Command::new("sh");
    command
        .args([
            "-c",
            "yes 'a\u{1D11E}' | head -c 3000000 && printf problem >&2 && exit 3",
        ])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let run = browser::finished(command, std::time::Duration::from_secs(20))
        .await
        .unwrap();
    assert_eq!(run.code, 3);
    assert!(run.output.ends_with("\n…"));
    assert!(run.output.len() <= browser::MOST_OUTPUT + "\n…".len());
    assert!(!run.output.contains(char::REPLACEMENT_CHARACTER));
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
            last_tab_closed: Box::new(|_| {}),
            restarted: Box::new(|_| {}),
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
