//! The displays that viewers can watch, as DXGI lists them, and where each one is on the desktop.

use screen::link::{Display, Sources};
use windows::Win32::{
    Foundation::RECT,
    Graphics::{
        Dxgi::{CreateDXGIFactory1, DXGI_OUTPUT_DESC, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput},
        Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO},
    },
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::MONITORINFOF_PRIMARY,
    },
};

/// Windows' dots per inch at a scale of 100 %, where a pixel is a point.
const POINT_DPI: f64 = 96.0;

/// A display and where it is.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Screen {
    pub(super) id: u32,
    /// Windows' name of the display, by which the capture finds it again.
    pub(super) device: String,
    /// Where the display is on the desktop, in pixels.
    pub(super) bounds: RECT,
    /// Pixels per point, from the scale that the person chose for the display.
    pub(super) scale: f64,
    primary: bool,
}

impl Screen {
    /// The desktop pixel at a point of the display, kept on the display.
    pub(super) fn pixel(&self, x: f64, y: f64) -> (f64, f64) {
        let bounds = self.bounds;
        let place = |start: i32, end: i32, at: f64| {
            at.mul_add(self.scale, f64::from(start))
                .clamp(f64::from(start), f64::from(end - 1))
        };
        (
            place(bounds.left, bounds.right, x),
            place(bounds.top, bounds.bottom, y),
        )
    }
}

/// The displays on the desktop now, the main one first.
pub(super) fn screens() -> Vec<Screen> {
    let mut screens: Vec<Screen> = outputs()
        .iter()
        .filter_map(|(_, output)| screen(output))
        .collect();
    screens.sort_by_key(|screen| !screen.primary);
    screens
}

pub(super) fn sources(screens: &[Screen]) -> Sources {
    Sources {
        displays: screens
            .iter()
            .map(|screen| Display {
                id: screen.id,
                width: f64::from(screen.bounds.right - screen.bounds.left) / screen.scale,
                height: f64::from(screen.bounds.bottom - screen.bounds.top) / screen.scale,
                scale: screen.scale,
            })
            .collect(),
        windows: Vec::new(),
    }
}

/// The outputs that show part of the desktop, with the adapter that each one is on.
pub(super) fn outputs() -> Vec<(IDXGIAdapter1, IDXGIOutput)> {
    // SAFETY: a factory of the helper's own.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    // SAFETY: enumerating stops at the first index that has no adapter.
    for adapter in (0..).map_while(|index| unsafe { factory.EnumAdapters1(index) }.ok()) {
        // SAFETY: enumerating stops at the first index that has no output.
        for output in (0..).map_while(|index| unsafe { adapter.EnumOutputs(index) }.ok()) {
            if describe(&output).is_some_and(|described| described.AttachedToDesktop.as_bool()) {
                found.push((adapter.clone(), output));
            }
        }
    }
    found
}

pub(super) fn describe(output: &IDXGIOutput) -> Option<DXGI_OUTPUT_DESC> {
    // SAFETY: a getter.
    unsafe { output.GetDesc() }.ok()
}

pub(super) fn device(described: &DXGI_OUTPUT_DESC) -> String {
    let name = &described.DeviceName;
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    String::from_utf16_lossy(&name[..end])
}

/// Pixels per point of a display.
pub(super) fn scale(monitor: HMONITOR) -> f64 {
    let (mut across, mut down) = (0, 0);
    // SAFETY: both pointers are valid to write.
    match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut across, &raw mut down) } {
        Ok(()) if across > 0 => f64::from(across) / POINT_DPI,
        _ => 1.0,
    }
}

fn screen(output: &IDXGIOutput) -> Option<Screen> {
    let described = describe(output)?;
    let device = device(&described);
    let mut monitor = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).ok()?,
        ..MONITORINFO::default()
    };
    // SAFETY: `monitor` is valid to write and says its size.
    let primary = unsafe { GetMonitorInfoW(described.Monitor, &raw mut monitor) }.as_bool()
        && monitor.dwFlags & MONITORINFOF_PRIMARY != 0;
    Some(Screen {
        id: number(&device)?,
        device,
        bounds: described.DesktopCoordinates,
        scale: scale(described.Monitor),
        primary,
    })
}

/// `\\.\DISPLAY2` is display 2, which stays its number while the displays change around it.
fn number(device: &str) -> Option<u32> {
    let digits = device.len() - device.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    device[device.len() - digits..].parse().ok()
}
