#![allow(unsafe_code)]

use std::{io, time::Duration};

use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_PROCESSED_OUTPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode,
    GetConsoleScreenBufferInfo, GetStdHandle, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    SetConsoleMode,
};

const POLL: Duration = Duration::from_millis(250);

fn mode(handle: STD_HANDLE) -> io::Result<u32> {
    let mut mode = 0;
    // SAFETY: `GetStdHandle` has no preconditions and `mode` is a valid output.
    if unsafe { GetConsoleMode(GetStdHandle(handle), &raw mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(mode)
}

fn set_mode(handle: STD_HANDLE, mode: u32) -> io::Result<()> {
    // SAFETY: `GetStdHandle` has no preconditions.
    if unsafe { SetConsoleMode(GetStdHandle(handle), mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) struct RawMode {
    input: u32,
    output: u32,
}

impl RawMode {
    pub(super) fn enter() -> io::Result<Self> {
        let saved = Self {
            input: mode(STD_INPUT_HANDLE)?,
            output: mode(STD_OUTPUT_HANDLE)?,
        };
        set_mode(STD_INPUT_HANDLE, ENABLE_VIRTUAL_TERMINAL_INPUT)?;
        set_mode(
            STD_OUTPUT_HANDLE,
            saved.output
                | ENABLE_PROCESSED_OUTPUT
                | ENABLE_VIRTUAL_TERMINAL_PROCESSING
                | DISABLE_NEWLINE_AUTO_RETURN,
        )?;
        Ok(saved)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        drop(set_mode(STD_INPUT_HANDLE, self.input));
        drop(set_mode(STD_OUTPUT_HANDLE, self.output));
    }
}

pub(super) fn size() -> io::Result<(u16, u16)> {
    // SAFETY: an all-zero structure is a valid output buffer.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a valid output for the console's screen buffer.
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let window = info.srWindow;
    let columns = u16::try_from(i32::from(window.Right) - i32::from(window.Left) + 1);
    let rows = u16::try_from(i32::from(window.Bottom) - i32::from(window.Top) + 1);
    columns
        .ok()
        .zip(rows.ok())
        .ok_or_else(|| io::Error::other("the console reported an invalid size"))
}

pub(super) struct Resizes {
    last: (u16, u16),
    ticks: tokio::time::Interval,
}

impl Resizes {
    pub(super) fn new() -> io::Result<Self> {
        let mut ticks = tokio::time::interval(POLL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Ok(Self {
            last: size()?,
            ticks,
        })
    }

    pub(super) async fn changed(&mut self) -> io::Result<(u16, u16)> {
        loop {
            self.ticks.tick().await;
            let current = size()?;
            if current != self.last {
                self.last = current;
                return Ok(current);
            }
        }
    }
}
