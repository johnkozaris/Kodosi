//! Only the people's tabs filter their requests, and a page itself always loads.

use std::{fs, path::PathBuf, sync::OnceLock};

use adblock::{
    Engine,
    lists::{FilterSet, ParseOptions},
    request::Request as Asked,
};
use cef::*;

static FILTERS: OnceLock<Engine> = OnceLock::new();

/// What a filter list calls each kind of request. A page's own document is never one.
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

/// Until the lists are read, every request goes through.
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

/// The lists: in the app's resources on a Mac, beside the program elsewhere.
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

pub(crate) fn blocker() -> ResourceRequestHandler {
    BlockerBuilder::new()
}

wrap_resource_request_handler! {
    struct BlockerBuilder;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let (Some(browser), Some(request)) = (browser, request) else {
                return ReturnValue::CONTINUE;
            };
            let page = browser
                .main_frame()
                .map(|frame| CefString::from(&frame.url()).to_string())
                .unwrap_or_default();
            let url = CefString::from(&request.url()).to_string();
            let method = CefString::from(&request.method()).to_string();
            if blocks(&page, &url, request.resource_type(), &method) {
                ReturnValue::CANCEL
            } else {
                ReturnValue::CONTINUE
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_itself_is_never_filtered_and_its_parts_have_their_list_names() {
        assert_eq!(kind(ResourceType::MAIN_FRAME), None);
        assert_eq!(kind(ResourceType::SCRIPT), Some("script"));
        assert_eq!(kind(ResourceType::XHR), Some("xmlhttprequest"));
    }

    #[test]
    fn a_listed_tracker_of_another_site_is_blocked_and_the_site_itself_is_not() {
        let mut set = FilterSet::new(false);
        set.add_filter_list(
            "||tracker.example^$third-party".to_owned(),
            ParseOptions::default(),
        );
        let engine = Engine::new_with_filter_set(set);
        let check = |url: &str| {
            let asked = Asked::new(url, "https://news.example/", "script", "GET").unwrap();
            engine.check_network_request(&asked).should_block()
        };
        assert!(check("https://tracker.example/t.js"));
        assert!(!check("https://news.example/app.js"));
    }
}
