//! The main thread: a hidden window that shows the notice in the notification area while people
//! watch, and hears when the session locks or changes hands, the displays sleep or change, and
//! something is copied.

use std::{
    cell::{OnceCell, RefCell},
    ffi::c_void,
};

use screen::Notice;
use tokio::sync::mpsc;
use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
            DeleteObject, HGDIOBJ,
        },
        System::{
            DataExchange::AddClipboardFormatListener,
            LibraryLoader::GetModuleHandleW,
            Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification},
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
            RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification},
            SystemServices::GUID_CONSOLE_DISPLAY_STATE,
        },
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                NIM_SETVERSION, NIN_SELECT, NINF_KEY, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW,
                DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DestroyIcon, DestroyMenu,
                DestroyWindow, DispatchMessageW, GetMessageW, GetSystemMetrics, HICON, ICONINFO,
                MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PBT_POWERSETTINGCHANGE, PostMessageW,
                PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SM_CXSMICON,
                SendMessageW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON,
                TrackPopupMenuEx, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CLIPBOARDUPDATE,
                WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_DISPLAYCHANGE, WM_NULL,
                WM_POWERBROADCAST, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_POPUP,
            },
        },
    },
    core::{PCWSTR, w},
};

use super::{
    clipboard, power,
    session::{self, Event},
};

/// A notice for the window, boxed in the message's `LPARAM`.
const NOTICE: u32 = WM_APP + 1;
/// What the person did with the notice's icon.
const NOTIFIED: u32 = WM_APP + 2;
/// Text for the clipboard, borrowed in the message's `LPARAM` while the sender waits.
const PUT_CLIPBOARD: u32 = WM_APP + 3;
/// The notice's icon chosen with the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const STOP: usize = 1;
const ICON: u32 = 1;
const CLASS: PCWSTR = w!("KodosiScreen");

thread_local! {
    static SHELL: OnceCell<Shell> = const { OnceCell::new() };
}

/// What the main thread keeps.
struct Shell {
    window: HWND,
    events: mpsc::UnboundedSender<Event>,
    notice: RefCell<Option<Notice>>,
    icon: RefCell<Option<Icon>>,
    /// Explorer's message when it starts again, which drops the icons it showed.
    taskbar_created: u32,
}

/// The helper's window, which other threads reach through its messages.
#[derive(Clone, Copy)]
pub(super) struct Window(usize);

impl Window {
    fn handle(self) -> HWND {
        HWND(self.0 as *mut c_void)
    }

    /// Shows who watches, or hides the notice once nobody does.
    pub(super) fn notice(self, notice: Option<Notice>) {
        let notice = Box::into_raw(Box::new(notice));
        // SAFETY: the window takes the box back from the message, and when the message does not
        // go, it comes back here.
        unsafe {
            if PostMessageW(
                Some(self.handle()),
                NOTICE,
                WPARAM(0),
                LPARAM(notice as isize),
            )
            .is_err()
            {
                drop(Box::from_raw(notice));
            }
        }
    }

    /// Puts text on the clipboard from the main thread, whose window then owns it, and returns
    /// once it is there.
    pub(super) fn put_clipboard(self, text: &str) {
        // SAFETY: the window reads the text during the call, which waits for it.
        unsafe {
            SendMessageW(
                self.handle(),
                PUT_CLIPBOARD,
                Some(WPARAM(0)),
                Some(LPARAM(std::ptr::from_ref(&text) as isize)),
            )
        };
    }

    fn close(self) {
        // SAFETY: a message to the helper's window.
        drop(unsafe { PostMessageW(Some(self.handle()), WM_CLOSE, WPARAM(0), LPARAM(0)) });
    }
}

pub(super) fn run(token: String) {
    let (say, saying) = mpsc::channel(session::MESSAGES);
    let (events, hearing) = mpsc::unbounded_channel();
    let Some(window) = open(events.clone()) else {
        return;
    };
    let linked = std::thread::Builder::new()
        .name("kodosi-screen-link".to_owned())
        .spawn(move || {
            session::serve(token, window, say, saying, events, hearing);
            window.close();
        });
    if linked.is_ok() {
        let mut message = MSG::default();
        // SAFETY: `message` is valid to write; `GetMessageW` returns 0 for `WM_QUIT` and -1 for
        // a failure.
        while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {
            // SAFETY: the message that came above.
            unsafe {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
    }
}

fn open(events: mpsc::UnboundedSender<Event>) -> Option<Window> {
    // SAFETY: the helper's own program.
    let instance = HINSTANCE::from(unsafe { GetModuleHandleW(None) }.ok()?);
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).ok()?,
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: CLASS,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: the class and its procedure last as long as the helper.
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        return None;
    }
    // SAFETY: a window of the class above, which never shows.
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS,
            w!("Kodosi"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .ok()?;
    // SAFETY: the name of a registered message.
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let shell = Shell {
        window,
        events,
        notice: RefCell::new(None),
        icon: RefCell::new(None),
        taskbar_created,
    };
    SHELL.with(|cell| cell.set(shell)).ok()?;
    // SAFETY: the window lasts until the helper quits, which ends what it hears.
    unsafe {
        drop(WTSRegisterSessionNotification(
            window,
            NOTIFY_FOR_THIS_SESSION,
        ));
        drop(RegisterPowerSettingNotification(
            HANDLE(window.0),
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        ));
        drop(AddClipboardFormatListener(window));
    }
    Some(Window(window.0 as usize))
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        NOTICE => {
            // SAFETY: `Window::notice` boxed the notice, which comes back once.
            let notice = unsafe { Box::from_raw(lparam.0 as *mut Option<Notice>) };
            with(|shell| shell.notice(*notice));
        }
        NOTIFIED => {
            let event = u32::try_from(lparam.0 & 0xffff).unwrap_or_default();
            if event == WM_CONTEXTMENU || event == NIN_SELECT || event == NIN_KEYSELECT {
                let (x, y) = (low(wparam.0), high(wparam.0));
                with(|shell| shell.menu(x, y));
            }
        }
        WM_COMMAND if wparam.0 & 0xffff == STOP => with(Shell::stop),
        PUT_CLIPBOARD => {
            // SAFETY: `Window::put_clipboard` lends its text while it waits for this.
            let text = unsafe { *(lparam.0 as *const &str) };
            clipboard::write(window, text);
        }
        WM_WTSSESSION_CHANGE | WM_DISPLAYCHANGE => with(Shell::changed),
        WM_POWERBROADCAST if wparam.0 == usize::try_from(PBT_POWERSETTINGCHANGE).unwrap_or(0) => {
            // SAFETY: a power setting's change comes with the setting and its data.
            let setting = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
            if setting.PowerSetting == GUID_CONSOLE_DISPLAY_STATE
                && usize::try_from(setting.DataLength).is_ok_and(|length| length >= 4)
            {
                // SAFETY: the display's state is a 32-bit number right after the length.
                let state = unsafe { setting.Data.as_ptr().cast::<u32>().read_unaligned() };
                power::display_changed(state);
                with(Shell::changed);
            }
            return LRESULT(1);
        }
        WM_CLIPBOARDUPDATE => with(Shell::clipboard_changed),
        WM_SETTINGCHANGE => with(Shell::repaint),
        WM_CLOSE => {
            // SAFETY: the helper's own window.
            drop(unsafe { DestroyWindow(window) });
        }
        WM_DESTROY => {
            with(|shell| shell.notice(None));
            // SAFETY: ends the main thread's loop.
            unsafe { PostQuitMessage(0) };
        }
        _ if SHELL.with(|cell| {
            cell.get()
                .is_some_and(|shell| shell.taskbar_created == message)
        }) =>
        {
            with(Shell::show_again);
        }
        // SAFETY: what the window does not handle.
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(0)
}

fn with(work: impl FnOnce(&Shell)) {
    SHELL.with(|cell| {
        if let Some(shell) = cell.get() {
            work(shell);
        }
    });
}

impl Shell {
    fn notice(&self, notice: Option<Notice>) {
        let shown = self.notice.borrow().is_some();
        match &notice {
            Some(notice) => {
                let data = self.data(Some(notice));
                // SAFETY: the data describes the helper's icon.
                unsafe {
                    if shown {
                        let _ = Shell_NotifyIconW(NIM_MODIFY, &raw const data);
                    } else {
                        let _ = Shell_NotifyIconW(NIM_ADD, &raw const data);
                        let _ = Shell_NotifyIconW(NIM_SETVERSION, &raw const data);
                    }
                }
            }
            None if shown => {
                let data = self.data(None);
                // SAFETY: as above.
                let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &raw const data) };
            }
            None => {}
        }
        *self.notice.borrow_mut() = notice;
    }

    fn data(&self, notice: Option<&Notice>) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or_default(),
            hWnd: self.window,
            uID: ICON,
            ..NOTIFYICONDATAW::default()
        };
        if let Some(notice) = notice {
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
            data.uCallbackMessage = NOTIFIED;
            data.hIcon = self.icon();
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let tip: Vec<u16> = notice
                .watching
                .encode_utf16()
                .take(data.szTip.len() - 1)
                .collect();
            data.szTip[..tip.len()].copy_from_slice(&tip);
        }
        data
    }

    /// An eye, as on a Mac, in the colour that shows on the taskbar.
    fn icon(&self) -> HICON {
        let light = light_taskbar();
        let mut icon = self.icon.borrow_mut();
        if icon.as_ref().is_none_or(|icon| icon.light != light) {
            *icon = Icon::eye(light);
        }
        icon.as_ref()
            .map_or_else(HICON::default, |icon| icon.handle)
    }

    /// Shows who watches and the button that ends the sharing. The menu opens a loop of its own,
    /// in which this window's messages go on.
    fn menu(&self, x: i32, y: i32) {
        let Some(notice) = self.notice.borrow().clone() else {
            return;
        };
        let watching = wide(&notice.watching);
        let stop = wide(&notice.stop);
        // SAFETY: a menu of the helper's own, which it destroys after use, with texts that
        // outlive it.
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            drop(AppendMenuW(
                menu,
                MF_STRING | MF_GRAYED,
                0,
                PCWSTR(watching.as_ptr()),
            ));
            drop(AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()));
            drop(AppendMenuW(menu, MF_STRING, STOP, PCWSTR(stop.as_ptr())));
            // Windows closes the menu when the person clicks elsewhere only while the menu's
            // window is in front.
            let _ = SetForegroundWindow(self.window);
            let _ = TrackPopupMenuEx(
                menu,
                (TPM_RIGHTBUTTON | TPM_BOTTOMALIGN).0,
                x,
                y,
                self.window,
                None,
            );
            drop(PostMessageW(
                Some(self.window),
                WM_NULL,
                WPARAM(0),
                LPARAM(0),
            ));
            drop(DestroyMenu(menu));
        }
    }

    fn stop(&self) {
        drop(self.events.send(Event::Stopped));
        self.notice(None);
    }

    fn changed(&self) {
        drop(self.events.send(Event::Changed));
    }

    fn clipboard_changed(&self) {
        drop(self.events.send(Event::ClipboardChanged));
    }

    /// The taskbar may have turned light or dark.
    fn repaint(&self) {
        let notice = self.notice.borrow().clone();
        if notice.is_some() {
            self.notice(notice);
        }
    }

    /// Explorer started again and lost the icon, which comes back.
    fn show_again(&self) {
        let notice = self.notice.borrow_mut().take();
        if notice.is_some() {
            self.notice(notice);
        }
    }
}

/// The notice's picture, drawn for the taskbar's colour.
struct Icon {
    handle: HICON,
    light: bool,
}

impl Icon {
    fn eye(light: bool) -> Option<Self> {
        // SAFETY: a getter.
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
        let pixels = eye(usize::try_from(size).ok()?, if light { 0 } else { 255 });
        let header = BITMAPINFOHEADER {
            biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..BITMAPINFO::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: the bitmaps are the helper's own and are deleted once the icon holds copies;
        // the pixels fill the colour bitmap exactly.
        unsafe {
            let colour = CreateDIBSection(
                None,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                None,
                0,
            )
            .ok()?;
            bits.cast::<u8>()
                .copy_from_nonoverlapping(pixels.as_ptr(), pixels.len());
            // The alpha above says what shows; a mask of zeros leaves it to the alpha. Its rows
            // are whole 16-bit words.
            let rows = usize::try_from(size).ok()?;
            let clear = vec![0_u8; rows.div_ceil(16) * 2 * rows];
            let mask = CreateBitmap(size, size, 1, 1, Some(clear.as_ptr().cast()));
            let parts = ICONINFO {
                fIcon: true.into(),
                hbmMask: mask,
                hbmColor: colour,
                ..ICONINFO::default()
            };
            let made = CreateIconIndirect(&raw const parts);
            let _ = DeleteObject(HGDIOBJ(colour.0));
            let _ = DeleteObject(HGDIOBJ(mask.0));
            Some(Self {
                handle: made.ok()?,
                light,
            })
        }
    }
}

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the helper made the icon and destroys it once.
        drop(unsafe { DestroyIcon(self.handle) });
    }
}

/// An eye of `size` pixels in BGRA rows, its outline and pupil in `ink` with soft edges.
fn eye(size: usize, ink: u8) -> Vec<u8> {
    let half = number(size) / 2.0;
    // Two circles whose overlap is the eye: as wide as 90 % of the icon, half as high.
    let (radius, offset) = (1.06, 0.56);
    let stroke = (1.4 / half).max(0.12);
    let pupil = 0.3;
    let mut pixels = Vec::with_capacity(size * size * 4);
    for row in 0..size {
        for column in 0..size {
            let x = (number(column) + 0.5 - half) / half;
            let y = (number(row) + 0.5 - half) / half;
            let lens = (x.hypot(y - offset) - radius).max(x.hypot(y + offset) - radius);
            let shape = (lens.abs() - stroke / 2.0).min(x.hypot(y) - pupil);
            let cover = shape.mul_add(-half, 0.5).clamp(0.0, 1.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the cover lies between 0 and 1"
            )]
            let alpha = (cover * 255.0).round() as u8;
            pixels.extend_from_slice(&[ink, ink, ink, alpha]);
        }
    }
    pixels
}

/// True when the taskbar is light, which a dark eye shows on.
fn light_taskbar() -> bool {
    let mut light = 0_u32;
    let mut size = u32::try_from(size_of::<u32>()).unwrap_or_default();
    // SAFETY: `light` holds the DWORD that the call is told to read.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut light).cast()),
            Some(&raw mut size),
        )
    };
    read == ERROR_SUCCESS && light == 1
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn number(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// The signed 16-bit halves of a message's parameter, as `GET_X_LPARAM` and `GET_Y_LPARAM` read.
fn low(parameter: usize) -> i32 {
    i32::from(i16::from_le_bytes([
        parameter.to_le_bytes()[0],
        parameter.to_le_bytes()[1],
    ]))
}

fn high(parameter: usize) -> i32 {
    i32::from(i16::from_le_bytes([
        parameter.to_le_bytes()[2],
        parameter.to_le_bytes()[3],
    ]))
}
