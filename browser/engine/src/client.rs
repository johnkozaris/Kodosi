use std::os::raw::c_int;

use cef::*;

use crate::{
    cursor,
    engine::{self, Origin, Shared},
    renderer,
};

/// The smallest screen that a page sees, like a laptop's, so that the screen is never just the
/// size of the page.
const SCREEN_WIDTH: u32 = 1920;
const SCREEN_HEIGHT: u32 = 1080;
const MENU_BAR: u32 = 25;

/// The crate names the cursor handle on Linux and Windows only.
#[cfg(target_os = "macos")]
type CursorHandle = *mut u8;

pub(crate) fn new(origin: Origin, shared: Shared) -> Client {
    ClientBuilder::new(
        RenderHandlerBuilder::new(shared),
        LifeSpanHandlerBuilder::new(origin),
        DisplayHandlerBuilder::new(),
        LoadHandlerBuilder::new(),
        ContextMenuHandlerBuilder::new(),
        RequestHandlerBuilder::new(),
    )
}

pub(crate) fn target_observer() -> DevToolsMessageObserver {
    TargetObserverBuilder::new()
}

fn id_of(browser: Option<&mut Browser>) -> Option<i64> {
    browser.map(|browser| i64::from(browser.identifier()))
}

wrap_client! {
    struct ClientBuilder {
        render: RenderHandler,
        life_span: LifeSpanHandler,
        display: DisplayHandler,
        load: LoadHandler,
        context_menu: ContextMenuHandler,
        requests: RequestHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(self.context_menu.clone())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(self.requests.clone())
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> c_int {
            let (Some(browser), Some(frame), Some(message)) = (id_of(browser), frame, message) else {
                return 0;
            };
            if CefString::from(&message.name()).to_string() != renderer::FROM_PAGE {
                return 0;
            }
            let Some(said) = message.argument_list().map(|list| CefString::from(&list.string(0)).to_string()) else {
                return 1;
            };
            let frame = CefString::from(&frame.identifier()).to_string();
            engine::with(|engine| engine.page_said(browser, frame, &said));
            1
        }
    }
}

wrap_render_handler! {
    struct RenderHandlerBuilder {
        shared: Shared,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let size = self.shared.size.get();
                rect.width = c_int::try_from(size.width).unwrap_or(c_int::MAX);
                rect.height = c_int::try_from(size.height).unwrap_or(c_int::MAX);
            }
        }

        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> c_int {
            let Some(info) = info else { return 0 };
            let size = self.shared.size.get();
            let width = c_int::try_from(size.width.max(SCREEN_WIDTH)).unwrap_or(c_int::MAX);
            let height = c_int::try_from(size.height.max(SCREEN_HEIGHT)).unwrap_or(c_int::MAX);
            info.device_scale_factor = size.scale;
            info.depth = 24;
            info.depth_per_component = 8;
            info.rect = Rect { x: 0, y: 0, width, height };
            info.available_rect = Rect {
                x: 0,
                y: c_int::try_from(MENU_BAR).unwrap_or(0),
                width,
                height: height - c_int::try_from(MENU_BAR).unwrap_or(0),
            };
            1
        }

        fn on_paint(
            &self,
            browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            let (Some(tab), Ok(width), Ok(height)) = (id_of(browser), usize::try_from(width), usize::try_from(height)) else {
                return;
            };
            if type_ != PaintElementType::VIEW || buffer.is_null() {
                return;
            }
            let mut capture = self.shared.capture.borrow_mut();
            let Some(capture) = capture.as_mut().filter(|capture| capture.tab == tab) else {
                return;
            };
            // SAFETY: CEF gives a buffer of `width` times `height` BGRA pixels that stays valid
            // until this call returns.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, width * height * 4) };
            capture.encoder.encode_pixels(pixels, width, height);
        }

        fn on_accelerated_paint(
            &self,
            browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            info: Option<&AcceleratedPaintInfo>,
        ) {
            let (Some(tab), Some(info)) = (id_of(browser), info) else {
                return;
            };
            if type_ != PaintElementType::VIEW {
                return;
            }
            let mut capture = self.shared.capture.borrow_mut();
            let Some(capture) = capture.as_mut().filter(|capture| capture.tab == tab) else {
                return;
            };
            #[cfg(target_os = "macos")]
            capture.encoder.encode_surface(info.shared_texture_io_surface);
            #[cfg(not(target_os = "macos"))]
            let _ = (capture, info);
        }
    }
}

wrap_life_span_handler! {
    struct LifeSpanHandlerBuilder {
        origin: Origin,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
            _popup_features: Option<&PopupFeatures>,
            window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            let (Some(opener), Some(window_info), Some(client)) = (id_of(browser), window_info, client) else {
                return 1;
            };
            let Some((origin, shared)) = engine::with(|engine| engine.place_popup(opener, target_disposition)).flatten() else {
                return 1;
            };
            *window_info = engine::window_info();
            *client = Some(new(origin, shared));
            0
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                engine::after_created(browser, self.origin);
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                engine::before_close(browser);
            }
        }
    }
}

wrap_display_handler! {
    struct DisplayHandlerBuilder;

    impl DisplayHandler {
        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let (Some(tab), Some(title)) = (id_of(browser), title) else { return };
            engine::with(|engine| engine.title(tab, title.to_string()));
        }

        fn on_address_change(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            let (Some(tab), Some(frame), Some(url)) = (id_of(browser), frame, url) else { return };
            if frame.is_main() == 1 {
                engine::with(|engine| engine.address(tab, url.to_string()));
            }
        }

        fn on_cursor_change(
            &self,
            browser: Option<&mut Browser>,
            _cursor: CursorHandle,
            type_: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> c_int {
            if let Some(tab) = id_of(browser) {
                engine::with(|engine| engine.shape(tab, cursor::css_name(type_)));
            }
            1
        }
    }
}

wrap_load_handler! {
    struct LoadHandlerBuilder;

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: c_int,
            _can_go_back: c_int,
            _can_go_forward: c_int,
        ) {
            if let Some(tab) = id_of(browser) {
                engine::with(|engine| engine.loading(tab, is_loading != 0));
            }
        }
    }
}

wrap_request_handler! {
    struct RequestHandlerBuilder;

    impl RequestHandler {
        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: c_int,
            _error_string: Option<&CefString>,
        ) {
            if let Some(browser) = browser {
                engine::page_process_ended(browser);
            }
        }
    }
}

wrap_context_menu_handler! {
    struct ContextMenuHandlerBuilder;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(model) = model {
                model.clear();
            }
            let (Some(tab), Some(params)) = (id_of(browser), params) else { return };
            let selection = CefString::from(&params.selection_text()).to_string();
            let link = CefString::from(&params.link_url()).to_string();
            let link = (!link.is_empty()).then_some(link);
            engine::with(|engine| engine.context(tab, selection, link));
        }
    }
}

wrap_dev_tools_message_observer! {
    struct TargetObserverBuilder;

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            browser: Option<&mut Browser>,
            _message_id: c_int,
            success: c_int,
            result: Option<&[u8]>,
        ) {
            let (Some(browser), Some(result)) = (id_of(browser), result) else { return };
            if success != 1 {
                return;
            }
            let target = serde_json::from_slice::<serde_json::Value>(result)
                .ok()
                .and_then(|result| result["targetInfo"]["targetId"].as_str().map(str::to_owned));
            if let Some(target) = target {
                engine::with(|engine| engine.target_known(browser, target));
            }
        }
    }
}
