use std::os::raw::c_int;

use cef::*;
use shared_browser::{Dialog, DialogKind};
use stream_encode::Picture;

#[cfg(target_os = "macos")]
use crate::cf::{IOSurfaceGetHeight, IOSurfaceGetWidth};
use crate::{
    cursor,
    engine::{self, Origin, Painting, Shared},
    filters, renderer,
};

/// Minimum virtual screen size reported to pages so the screen is not shrunk to the viewport.
const SCREEN_WIDTH: u32 = 1920;
const SCREEN_HEIGHT: u32 = 1080;
const MENU_BAR: c_int = 25;
/// Favicons render at 16pt in tabs, so request 32px for 2x displays.
const ICON_PIXELS: u32 = 32;

/// `cef` only defines `CursorHandle` on Linux and Windows.
#[cfg(target_os = "macos")]
type CursorHandle = *mut u8;

/// Host pages only load their own files, so request filtering applies only to tabs.
pub(crate) fn new(origin: Origin, shared: Shared) -> Client {
    let blocker = matches!(origin, Origin::Tab { .. }).then(filters::blocker);
    ClientBuilder::new(
        RenderHandlerBuilder::new(shared),
        LifeSpanHandlerBuilder::new(origin),
        DisplayHandlerBuilder::new(),
        LoadHandlerBuilder::new(),
        ContextMenuHandlerBuilder::new(),
        RequestHandlerBuilder::new(blocker),
        DialogHandlerBuilder::new(),
    )
}

pub(crate) fn target_observer() -> DevToolsMessageObserver {
    TargetObserverBuilder::new()
}

fn id_of(browser: Option<&mut Browser>) -> Option<i64> {
    browser.map(|browser| i64::from(browser.identifier()))
}

/// Reads the first URL and frees its CEF string copy manually, working around `cef-rs#516`
/// where `CefStringList`'s iterator leaks each copied string.
fn first_address(urls: &mut CefStringList) -> Option<String> {
    let list: *mut cef_dll_sys::_cef_string_list_t = urls.into();
    if list.is_null() {
        return None;
    }
    let mut value = cef_dll_sys::cef_string_t {
        str_: std::ptr::null_mut(),
        length: 0,
        dtor: None,
    };
    // SAFETY: CEF keeps the list valid for this call, and `value` owns the copy that
    // `cef_string_list_value` makes until `cef_string_utf16_clear` frees it.
    unsafe {
        if cef_dll_sys::cef_string_list_size(list) == 0
            || cef_dll_sys::cef_string_list_value(list, 0, &raw mut value) == 0
        {
            return None;
        }
        let address = CefString::from(&raw const value).to_string();
        cef_dll_sys::cef_string_utf16_clear(&raw mut value);
        Some(address)
    }
}

wrap_client! {
    struct ClientBuilder {
        render: RenderHandler,
        life_span: LifeSpanHandler,
        display: DisplayHandler,
        load: LoadHandler,
        context_menu: ContextMenuHandler,
        requests: RequestHandler,
        dialogs: JsdialogHandler,
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

        fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
            Some(self.dialogs.clone())
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
                y: MENU_BAR,
                width,
                height: height - MENU_BAR,
            };
            1
        }

        fn on_paint(
            &self,
            browser: Option<&mut Browser>,
            type_: PaintElementType,
            dirty_rects: Option<&[Rect]>,
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
            // SAFETY: CEF provides a `width * height` BGRA buffer that remains valid until
            // this callback returns.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, width * height * 4) };
            #[cfg(target_os = "macos")]
            if let Some(layer) = self.shared.layer.borrow_mut().as_mut().filter(|layer| layer.tab == Some(tab)) {
                layer.draw_pixels(pixels, width, height, self.shared.size.get());
            }
            let mut capture = self.shared.capture.borrow_mut();
            if let Some(capture) = capture.as_mut().filter(|capture| capture.tab == tab)
                && capture.takes(width, height, Painting::Memory)
            {
                let changed = changed(dirty_rects, &self.shared);
                capture.encoder.encode(
                    Picture::Pixels {
                        bgra: pixels,
                        width,
                        height,
                    },
                    None,
                    changed,
                );
            }
        }

        fn on_accelerated_paint(
            &self,
            browser: Option<&mut Browser>,
            type_: PaintElementType,
            dirty_rects: Option<&[Rect]>,
            info: Option<&AcceleratedPaintInfo>,
        ) {
            let (Some(tab), Some(info)) = (id_of(browser), info) else {
                return;
            };
            if type_ != PaintElementType::VIEW {
                return;
            }
            #[cfg(target_os = "macos")]
            {
                let surface = info.shared_texture_io_surface;
                if surface.is_null() {
                    return;
                }
                if let Some(layer) = self.shared.layer.borrow_mut().as_mut().filter(|layer| layer.tab == Some(tab)) {
                    layer.draw_surface(surface, self.shared.size.get());
                }
                // SAFETY: CEF provides a valid IOSurface for the duration of this paint callback.
                let (width, height) = unsafe { (IOSurfaceGetWidth(surface), IOSurfaceGetHeight(surface)) };
                let mut capture = self.shared.capture.borrow_mut();
                if let Some(capture) = capture.as_mut().filter(|capture| capture.tab == tab)
                    && capture.takes(width, height, Painting::Surface)
                {
                    let changed = changed(dirty_rects, &self.shared);
                    capture.encoder.encode(Picture::Surface(surface), None, changed);
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (tab, info, dirty_rects);
        }
    }
}

/// The share of the page that `dirty_rects` cover. CEF reports them in physical pixels, so the
/// page's size is scaled to match.
fn changed(dirty_rects: Option<&[Rect]>, shared: &Shared) -> f64 {
    let size = shared.size.get();
    let pixels = |points: u32| f64::from(points) * f64::from(size.scale);
    let page = pixels(size.width) * pixels(size.height);
    let changed = dirty_rects.map_or(page, |rects| {
        rects
            .iter()
            .map(|rect| f64::from(rect.width.max(0)) * f64::from(rect.height.max(0)))
            .sum()
    });
    (changed / page.max(1.0)).min(1.0)
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

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let Some(browser) = browser else { return };
            let tab = i64::from(browser.identifier());
            let address = icon_urls.and_then(first_address).unwrap_or_default();
            if engine::with(|engine| engine.icon_named(tab, &address)) == Some(true)
                && let Some(host) = browser.host()
            {
                let mut fetched = IconBuilder::new(tab);
                host.download_image(Some(&CefString::from(address.as_str())), 1, ICON_PIXELS, 0, Some(&mut fetched));
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

wrap_download_image_callback! {
    struct IconBuilder {
        tab: i64,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(&self, image_url: Option<&CefString>, _http_status_code: c_int, image: Option<&mut Image>) {
            let address = image_url.map_or_default(CefString::to_string);
            let png = image
                .and_then(|image| {
                    // CEF requires non-null width/height out pointers or `as_png` returns None.
                    let (mut width, mut height) = (0, 0);
                    image.as_png(1.0, 1, Some(&mut width), Some(&mut height))
                })
                .map_or_default(|png| {
                    let mut bytes = vec![0; png.size()];
                    png.data(Some(&mut bytes), 0);
                    bytes
                });
            engine::with(|engine| engine.icon(self.tab, &address, &png));
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
    struct RequestHandlerBuilder {
        blocker: Option<ResourceRequestHandler>,
    }

    impl RequestHandler {
        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: c_int,
            _is_download: c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut c_int>,
        ) -> Option<ResourceRequestHandler> {
            self.blocker.clone()
        }

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

/// Forwards page JS dialogs to the host so the engine never opens native dialog windows.
fn take_dialog(
    browser: Option<&mut Browser>,
    kind: DialogKind,
    message: Option<&CefString>,
    text: Option<&CefString>,
    callback: Option<&mut JsdialogCallback>,
) -> bool {
    let (Some(tab), Some(callback)) = (id_of(browser), callback) else {
        return false;
    };
    let words = |text: Option<&CefString>| text.map_or_default(CefString::to_string);
    let dialog = Dialog {
        kind,
        message: words(message),
        text: words(text),
    };
    engine::with(|engine| engine.dialog_opened(tab, dialog, callback.clone())).unwrap_or(false)
}

wrap_jsdialog_handler! {
    struct DialogHandlerBuilder;

    impl JsdialogHandler {
        fn on_jsdialog(
            &self,
            browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            dialog_type: JsdialogType,
            message_text: Option<&CefString>,
            default_prompt_text: Option<&CefString>,
            callback: Option<&mut JsdialogCallback>,
            suppress_message: Option<&mut c_int>,
        ) -> c_int {
            let kind = match dialog_type {
                JsdialogType::CONFIRM => DialogKind::Confirm,
                JsdialogType::PROMPT => DialogKind::Prompt,
                _ => DialogKind::Alert,
            };
            if take_dialog(browser, kind, message_text, default_prompt_text, callback) {
                return 1;
            }
            if let Some(suppress) = suppress_message {
                *suppress = 1;
            }
            0
        }

        fn on_before_unload_dialog(
            &self,
            browser: Option<&mut Browser>,
            message_text: Option<&CefString>,
            _is_reload: c_int,
            callback: Option<&mut JsdialogCallback>,
        ) -> c_int {
            let mut callback = callback.cloned();
            if !take_dialog(browser, DialogKind::Beforeunload, message_text, None, callback.as_mut())
                && let Some(callback) = callback
            {
                callback.cont(1, None);
            }
            1
        }

        fn on_reset_dialog_state(&self, browser: Option<&mut Browser>) {
            if let Some(tab) = id_of(browser) {
                engine::with(|engine| engine.take_dialog(tab));
            }
        }

        fn on_dialog_closed(&self, browser: Option<&mut Browser>) {
            if let Some(tab) = id_of(browser) {
                engine::with(|engine| engine.take_dialog(tab));
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
