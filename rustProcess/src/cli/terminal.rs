use crate::{
    Error, Result,
    headless::{self, TerminalFrame},
};
use ghostty_vt::{CheckpointLimits, SemanticCheckpoint, Terminal, TerminalPolicy};
use std::{
    fs::{File, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::Duration,
};
use tokio::io::unix::AsyncFd;
use uuid::Uuid;

struct ScreenGuard;
impl ScreenGuard {
    fn enter() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        if let Err(error) = io::stdout().write_all(b"\x1b[?1049h") {
            drop(crossterm::terminal::disable_raw_mode());
            return Err(error.into());
        }
        Ok(Self)
    }
}
impl Drop for ScreenGuard {
    fn drop(&mut self) {
        drop(crossterm::terminal::disable_raw_mode());
        drop(io::stdout().write_all(b"\x1b[?1049l"));
        drop(io::stdout().flush());
    }
}

#[expect(
    clippy::future_not_send,
    reason = "Ghostty remains on the CLI main thread"
)]
pub(super) async fn attach(root: &Path, session: Uuid) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::Invalid(
            "terminal attach requires an interactive terminal".into(),
        ));
    }
    let mut client = headless::TerminalClient::connect(root, session).await?;
    let initial = tokio::time::timeout(
        Duration::from_secs(5),
        headless::read_terminal(&mut client.reader),
    )
    .await
    .map_err(|_| Error::Other("terminal snapshot timed out".into()))??;
    let TerminalFrame::Checkpoint {
        bytes,
        rows,
        cols,
        next_sequence,
    } = initial
    else {
        return Err(Error::Invalid(
            "terminal stream did not start with a snapshot".into(),
        ));
    };
    let mut mirror = restore(&bytes, rows, cols)?;
    let mut next = next_sequence;
    let _screen = ScreenGuard::enter()?;
    render(&mut mirror)?;
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        headless::write_resize(&mut client.writer, cols, rows).await?;
    }
    let stdin = AsyncFd::new(
        OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed())
            .open("/dev/tty")?,
    )?;
    let mut buffer = [0u8; 4096];
    let mut escaped = false;
    let mut resize = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
    let mut paint = tokio::time::interval(Duration::from_millis(16));
    paint.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = false;
    loop {
        tokio::select! {
            input=read_input(&stdin, &mut buffer)=>{
                let count=input?;if count==0{return Ok(());}
                let mut outgoing=Vec::with_capacity(count+1);
                for byte in &buffer[..count]{
                    if escaped{escaped=false;if *byte==b'.'{return Ok(());}outgoing.push(0x1d);}
                    if *byte==0x1d{escaped=true;}else{outgoing.push(*byte);}
                }
                if !outgoing.is_empty(){headless::write_input(&mut client.writer,&outgoing).await?;}
            }
            frame=headless::read_terminal(&mut client.reader)=>match frame?{
                TerminalFrame::InputAck { accepted, message } => { if !accepted { return Err(Error::Other(message.unwrap_or_else(|| "Input was rejected".into()))); } },
                TerminalFrame::Checkpoint{..}=>return Err(Error::Invalid("unexpected terminal snapshot on established stream".into())),
                TerminalFrame::Data{bytes,sequence}=>{
                    if sequence<next{continue;}
                    if sequence!=next{return Err(Error::Invalid("terminal stream lost output; reconnect to restore it".into()));}
                    drop(mirror.write(&bytes).map_err(|e|Error::Other(e.to_string()))?);
                    next=next.checked_add(1).ok_or_else(||Error::Invalid("terminal sequence exhausted".into()))?;dirty=true;
                }
                TerminalFrame::Resize{rows,cols,at_sequence}=>{
                    if at_sequence!=next{return Err(Error::Invalid("terminal resize crossed its output boundary".into()));}
                    drop(mirror.resize(cols,rows,0,0).map_err(|e|Error::Other(e.to_string()))?);dirty=true;
                }
                TerminalFrame::Closed{reason,final_sequence}=>{
                    if next!=final_sequence{return Err(Error::Other(format!("terminal ended before final output: {reason}")));}
                    if dirty{render(&mut mirror)?;}return Ok(());
                }
            },
            _=resize.recv()=>{if let Ok((cols,rows))=crossterm::terminal::size(){headless::write_resize(&mut client.writer,cols,rows).await?;}},
            _=paint.tick(),if dirty=>{render(&mut mirror)?;dirty=false;}
        }
    }
}
async fn read_input(input: &AsyncFd<File>, bytes: &mut [u8]) -> io::Result<usize> {
    loop {
        let mut ready = input.readable().await?;
        if let Ok(result) = ready.try_io(|input| input.get_ref().read(bytes)) {
            return result;
        }
    }
}

fn restore(bytes: &[u8], rows: u16, cols: u16) -> Result<Terminal> {
    let mut terminal = Terminal::new(cols, rows, TerminalPolicy::default())
        .map_err(|e| Error::Other(e.to_string()))?;
    terminal
        .restore_semantic_checkpoint(
            &SemanticCheckpoint::from(bytes.to_vec()),
            CheckpointLimits::default(),
        )
        .map_err(|e| Error::Other(e.to_string()))?;
    let state = terminal.state().map_err(|e| Error::Other(e.to_string()))?;
    if state.rows != rows || state.cols != cols {
        return Err(Error::Invalid(
            "terminal snapshot geometry does not match".into(),
        ));
    }
    Ok(terminal)
}
fn render(terminal: &mut Terminal) -> Result<()> {
    let bytes = terminal
        .format_finite_cli_replay()
        .map_err(|e| Error::Other(e.to_string()))?;
    let mut output = io::stdout().lock();
    output.write_all(b"\x1b[?2026h")?;
    let result = output.write_all(&bytes);
    let ended = output.write_all(b"\x1b[?2026l");
    result?;
    ended?;
    output.flush()?;
    Ok(())
}
