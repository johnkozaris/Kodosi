use std::{fs, path::PathBuf, sync::OnceLock};

use adblock::{
    Engine,
    lists::{FilterSet, ParseOptions},
    request::Request as Asked,
};
use cef::*;

static FILTERS: OnceLock<Engine> = OnceLock::new();

/// Maps CEF resource types to adblock filter types. `MAIN_FRAME` is intentionally omitted so
/// top-level page navigations are never blocked.
const KINDS: [(ResourceType, &str); 18] = [
    (ResourceType::SUB_FRAME, "subdocument"),
    (ResourceType::NAVIGATION_PRELOAD_SUB_FRAME, "subdocument"),
    (ResourceType::STYLESHEET, "stylesheet"),
    (ResourceType::SCRIPT, "script"),
    (ResourceType::WORKER, "script"),
    (ResourceType::SHARED_WORKER, "script"),
    (ResourceType::SERVICE_WORKER, "script"),
    (ResourceType::IMAGE, "image"),
    (ResourceType::FAVICON, "image"),
    (ResourceType::FONT_RESOURCE, "font"),
    (ResourceType::OBJECT, "object"),
    (ResourceType::PLUGIN_RESOURCE, "object"),
    (ResourceType::MEDIA, "media"),
    (ResourceType::XHR, "xmlhttprequest"),
    (ResourceType::PING, "ping"),
    (ResourceType::CSP_REPORT, "csp_report"),
    (ResourceType::SUB_RESOURCE, "other"),
    (ResourceType::PREFETCH, "other"),
];

pub(crate) fn load() {
    let Some(folder) = folder() else {
        return;
    };
    let started = std::thread::Builder::new()
        .name("kodosi-filters".to_owned())
        .spawn(move || {
            let Ok(lists) = fs::read_dir(folder) else {
                return;
            };
            let mut set = FilterSet::new(false);
            for list in lists.flatten() {
                if let Ok(text) = fs::read_to_string(list.path()) {
                    set.add_filter_list(text, ParseOptions::default());
                }
            }
            drop(FILTERS.set(Engine::new_with_filter_set(set)));
        });
    drop(started);
}

fn folder() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    let beside = program.parent()?;
    Some(if cfg!(target_os = "macos") {
        beside.parent()?.join("Resources").join("filters")
    } else {
        beside.join("filters")
    })
}

fn kind(resource: ResourceType) -> Option<&'static str> {
    KINDS
        .iter()
        .find(|(known, _)| *known == resource)
        .map(|(_, kind)| *kind)
}

fn blocks(page: &str, url: &str, resource: ResourceType, method: &str) -> bool {
    let (Some(filters), Some(kind)) = (FILTERS.get(), kind(resource)) else {
        return false;
    };
    Asked::new(url, page, kind, method)
        .is_ok_and(|asked| filters.check_network_request(&asked).should_block())
}

pub(crate) fn blocks_request(browser: Option<&mut Browser>, request: Option<&mut Request>) -> bool {
    let (Some(browser), Some(request)) = (browser, request) else {
        return false;
    };
    let page = browser
        .main_frame()
        .map_or_default(|frame| CefString::from(&frame.url()).to_string());
    let url = CefString::from(&request.url()).to_string();
    let method = CefString::from(&request.method()).to_string();
    blocks(&page, &url, request.resource_type(), &method)
}

pub(crate) fn blocker() -> ResourceRequestHandler {
    BlockerBuilder::new()
}

wrap_resource_request_handler! {
    struct BlockerBuilder;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CANCEL
        }
    }
}
