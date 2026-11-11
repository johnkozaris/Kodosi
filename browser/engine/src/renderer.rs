use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    os::raw::c_int,
};

use cef::*;
use serde::Deserialize;

pub(crate) const FROM_PAGE: &str = "kodosi";
const TO_PAGE: &str = "kodosi-pick";
/// Marks a host page, where no page script runs.
pub(crate) const HOST_PAGE: &str = "kodosiHostPage";
const SCRIPT: &str = include_str!("page.js");
const SCRIPT_ADDRESS: &str = "kodosi://page.js";

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum Said {
    Menu { menu: u32, items: Vec<SaidItem> },
    Copied { text: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct SaidItem {
    pub label: String,
    pub enabled: bool,
    pub chosen: bool,
}

thread_local! {
    static HOST_PAGES: RefCell<HashSet<c_int>> = RefCell::default();
    static LISTENERS: RefCell<HashMap<String, V8Value>> = RefCell::default();
}

pub(crate) fn handler() -> RenderProcessHandler {
    RenderProcessHandlerBuilder::new()
}

pub(crate) fn pick(browser: &Browser, frame: &str, menu: u32, choice: Option<u32>) {
    let Some(frame) = browser.frame_by_identifier(Some(&CefString::from(frame))) else {
        return;
    };
    let mut message = process_message_create(Some(&TO_PAGE.into()));
    if let Some(list) = message.as_ref().and_then(ProcessMessage::argument_list) {
        list.set_int(0, c_int::try_from(menu).unwrap_or(-1));
        list.set_int(
            1,
            choice
                .and_then(|choice| c_int::try_from(choice).ok())
                .unwrap_or(-1),
        );
    }
    frame.send_process_message(ProcessId::RENDERER, message.as_mut());
}

fn frame_id(frame: &Frame) -> String {
    CefString::from(&frame.identifier()).to_string()
}

wrap_render_process_handler! {
    struct RenderProcessHandlerBuilder;

    impl RenderProcessHandler {
        fn on_browser_created(&self, browser: Option<&mut Browser>, extra_info: Option<&mut DictionaryValue>) {
            let (Some(browser), Some(extra_info)) = (browser, extra_info) else { return };
            if extra_info.bool(Some(&HOST_PAGE.into())) == 1 {
                HOST_PAGES.with(|pages| pages.borrow_mut().insert(browser.identifier()));
            }
        }

        fn on_browser_destroyed(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                HOST_PAGES.with(|pages| pages.borrow_mut().remove(&browser.identifier()));
            }
        }

        fn on_context_created(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, context: Option<&mut V8Context>) {
            let (Some(browser), Some(frame), Some(context)) = (browser, frame, context) else { return };
            if HOST_PAGES.with(|pages| pages.borrow().contains(&browser.identifier())) {
                return;
            }
            let mut send = SendBuilder::new(frame.clone());
            let mut listen = ListenBuilder::new(frame_id(frame));
            let send = v8_value_create_function(Some(&"send".into()), Some(&mut send));
            let listen = v8_value_create_function(Some(&"listen".into()), Some(&mut listen));
            let mut factory = None;
            let mut exception = None;
            context.eval(
                Some(&SCRIPT.into()),
                Some(&SCRIPT_ADDRESS.into()),
                1,
                Some(&mut factory),
                Some(&mut exception),
            );
            if let Some(factory) = factory.filter(|factory| factory.is_function() == 1) {
                factory.execute_function_with_context(Some(context), None, Some(&[send, listen]));
            }
        }

        fn on_context_released(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _context: Option<&mut V8Context>) {
            if let Some(frame) = frame {
                let id = frame_id(frame);
                LISTENERS.with(|listeners| listeners.borrow_mut().remove(&id));
            }
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> c_int {
            let (Some(frame), Some(message)) = (frame, message) else { return 0 };
            if CefString::from(&message.name()).to_string() != TO_PAGE {
                return 0;
            }
            let Some(list) = message.argument_list() else { return 1 };
            let listener = LISTENERS.with(|listeners| listeners.borrow().get(&frame_id(frame)).cloned());
            let (Some(listener), Some(mut context)) = (listener, frame.v8_context()) else { return 1 };
            let arguments = [v8_value_create_int(list.int(0)), v8_value_create_int(list.int(1))];
            listener.execute_function_with_context(Some(&mut context), None, Some(&arguments));
            1
        }
    }
}

wrap_v8_handler! {
    struct SendBuilder {
        frame: Frame,
    }

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> c_int {
            let text = arguments
                .and_then(<[_]>::first)
                .and_then(Option::as_ref)
                .filter(|value| value.is_string() == 1)
                .map(|value| CefString::from(&value.string_value()).to_string());
            let Some(text) = text else { return 1 };
            let mut message = process_message_create(Some(&FROM_PAGE.into()));
            if let Some(list) = message.as_ref().and_then(ProcessMessage::argument_list) {
                list.set_string(0, Some(&text.as_str().into()));
            }
            self.frame.send_process_message(ProcessId::BROWSER, message.as_mut());
            1
        }
    }
}

wrap_v8_handler! {
    struct ListenBuilder {
        frame: String,
    }

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> c_int {
            let listener = arguments
                .and_then(<[_]>::first)
                .and_then(Option::as_ref)
                .filter(|value| value.is_function() == 1)
                .cloned();
            if let Some(listener) = listener {
                LISTENERS.with(|listeners| listeners.borrow_mut().insert(self.frame.clone(), listener));
            }
            1
        }
    }
}
