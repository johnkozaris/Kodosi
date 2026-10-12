use std::{
    collections::{BTreeMap, HashMap},
    ffi::{OsStr, OsString, c_void},
    io,
    os::windows::{
        ffi::{OsStrExt as _, OsStringExt as _},
        io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{Mutex, OnceLock, PoisonError, mpsc as std_mpsc},
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};
use windows_sys::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, ERROR_MORE_DATA, HANDLE, WAIT_OBJECT_0},
    Storage::FileSystem::{ReadFile, WriteFile},
    System::{
        Console::{COORD, HPCON, SetConsoleCtrlHandler},
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_PROCESS_ID_LIST,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicProcessIdList,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
            TerminateJobObject,
        },
        LibraryLoader::{
            GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
            LoadLibraryExW,
        },
        Pipes::CreatePipe,
        Threading::{
            CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess, PROC_THREAD_ATTRIBUTE_JOB_LIST,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, STARTF_USESTDHANDLES,
            STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

use super::{
    ShutdownStage, Transfer, WaitOutcome, inherits_foreign_context, program_identity,
    validate_working_dir,
};
use crate::{KodosiError, Result};

/// `ConPTY` measures emoji and other grapheme clusters as `libghostty-vt` does.
const PSEUDOCONSOLE_GLYPH_WIDTH_GRAPHEMES: u32 = 0x08;
const READ_BYTES: usize = 64 * 1024;
const THREAD_STACK: usize = 64 * 1024;
const POLL: Duration = Duration::from_millis(20);
const MAX_JOB_PROCESSES: usize = 4096;
/// Shells whose programs run where Windows cannot see them.
const OPAQUE_SHELLS: [&str; 2] = ["ssh", "wsl"];

type CreateFn = unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> i32;
type ResizeFn = unsafe extern "system" fn(HPCON, COORD) -> i32;
type ReleaseFn = unsafe extern "system" fn(HPCON) -> i32;
type CloseFn = unsafe extern "system" fn(HPCON);

/// The bundled `conpty.dll`, which runs its own `OpenConsole.exe`.
#[derive(Debug)]
struct Conpty {
    create: CreateFn,
    resize: ResizeFn,
    release: ReleaseFn,
    close: CloseFn,
}

fn conpty() -> Result<&'static Conpty> {
    static API: OnceLock<std::result::Result<Conpty, String>> = OnceLock::new();
    API.get_or_init(load_conpty)
        .as_ref()
        .map_err(|message| KodosiError::Spawn(message.clone()))
}

fn conpty_directory() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .filter(|directory| directory.join("conpty.dll").is_file());
    beside.or_else(|| {
        std::env::var_os("KODOSI_CONPTY_DIR")
            .map(PathBuf::from)
            .filter(|directory| directory.is_absolute() && directory.join("conpty.dll").is_file())
    })
}

fn load_conpty() -> std::result::Result<Conpty, String> {
    let path = conpty_directory()
        .ok_or("conpty.dll is missing; reinstall Kodosi")?
        .join("conpty.dll");
    // SAFETY: the path is absolute and NUL-terminated. The module stays loaded for the process.
    let module = unsafe {
        LoadLibraryExW(
            wide(path.as_os_str()).as_ptr(),
            ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        return Err(format!(
            "could not load {}: {}",
            path.display(),
            io::Error::last_os_error()
        ));
    }
    let symbol = |name: &'static [u8]| {
        // SAFETY: the module is loaded and the name is NUL-terminated.
        unsafe { GetProcAddress(module, name.as_ptr()) }.ok_or_else(|| {
            format!(
                "conpty.dll has no {}",
                String::from_utf8_lossy(&name[..name.len() - 1])
            )
        })
    };
    // SAFETY: each export has the signature that conpty.h declares.
    unsafe {
        Ok(Conpty {
            create: std::mem::transmute::<unsafe extern "system" fn() -> isize, CreateFn>(symbol(
                b"ConptyCreatePseudoConsole\0",
            )?),
            resize: std::mem::transmute::<unsafe extern "system" fn() -> isize, ResizeFn>(symbol(
                b"ConptyResizePseudoConsole\0",
            )?),
            release: std::mem::transmute::<unsafe extern "system" fn() -> isize, ReleaseFn>(
                symbol(b"ConptyReleasePseudoConsole\0")?,
            ),
            close: std::mem::transmute::<unsafe extern "system" fn() -> isize, CloseFn>(symbol(
                b"ConptyClosePseudoConsole\0",
            )?),
        })
    }
}

fn hresult(code: i32) -> io::Result<()> {
    if code >= 0 {
        return Ok(());
    }
    let code = code.cast_unsigned();
    Err(if code & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((code & 0xFFFF).cast_signed())
    } else {
        io::Error::other(format!("ConPTY failed with 0x{code:08X}"))
    })
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn size(rows: u16, cols: u16) -> COORD {
    COORD {
        X: i16::try_from(cols.max(1)).unwrap_or(i16::MAX),
        Y: i16::try_from(rows.max(1)).unwrap_or(i16::MAX),
    }
}

/// An open pseudoconsole. Dropping it closes the console without waiting for clients.
#[derive(Debug)]
struct Console {
    api: &'static Conpty,
    handle: HPCON,
}

impl Drop for Console {
    fn drop(&mut self) {
        // SAFETY: the handle came from `ConptyCreatePseudoConsole` and is closed once.
        unsafe { (self.api.close)(self.handle) };
    }
}

#[derive(Debug)]
struct Write {
    bytes: Vec<u8>,
    done: Option<oneshot::Sender<io::Result<usize>>>,
}

#[derive(Debug, Default)]
struct Observed {
    identities: HashMap<u32, Option<&'static str>>,
    busy: bool,
    period: u32,
}

struct Observation {
    others: Vec<u32>,
    agent: Option<(u32, &'static str)>,
    period: u32,
}

#[derive(Debug)]
pub struct KodosiPty {
    console: Option<Console>,
    job: OwnedHandle,
    process: OwnedHandle,
    child_pid: u32,
    input: std_mpsc::Sender<Write>,
    opaque: bool,
    observed: Mutex<Observed>,
}

impl KodosiPty {
    pub fn spawn_program(
        program: &Path,
        arguments: &[String],
        working_dir: Option<&str>,
        rows: u16,
        cols: u16,
        environment: &[(&str, &str)],
    ) -> Result<(Self, PtyReader)> {
        let api = conpty()?;
        let working_dir = validate_working_dir(working_dir)?;
        let program = resolve_program(program, working_dir)?;
        let (application, mut line) = command_line(&program, arguments)?;
        let environment = environment_block(environment);
        let job = create_job()?;
        let (input_read, input_write) = pipe()?;
        let (output_read, output_write) = pipe()?;
        let mut handle: HPCON = 0;
        // SAFETY: the pipe handles are open and `handle` receives the new console.
        hresult(unsafe {
            (api.create)(
                size(rows, cols),
                input_read.as_raw_handle(),
                output_write.as_raw_handle(),
                PSEUDOCONSOLE_GLYPH_WIDTH_GRAPHEMES,
                &raw mut handle,
            )
        })?;
        let console = Console { api, handle };
        drop((input_read, output_write));
        let (process, child_pid) = start(
            &console,
            &job,
            &application,
            &mut line,
            &environment,
            working_dir,
        )?;
        // Without our reference, the console ends when its last client does,
        // which closes the output pipe.
        // SAFETY: the console is open; a failed release only delays the end of output.
        drop(hresult(unsafe { (api.release)(console.handle) }));
        let (input, requests) = std_mpsc::channel();
        spawn_writer(input_write, requests)?;
        let output = spawn_reader(output_read)?;
        let opaque = program
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| {
                OPAQUE_SHELLS
                    .iter()
                    .any(|shell| shell.eq_ignore_ascii_case(stem))
            })
            || is_system_bash(&program);
        Ok((
            Self {
                console: Some(console),
                job,
                process,
                child_pid,
                input: input.clone(),
                opaque,
                observed: Mutex::new(Observed::default()),
            },
            PtyReader {
                output,
                chunk: Vec::new(),
                offset: 0,
                input,
                in_flight: None,
            },
        ))
    }

    /// Windows has no supported way to read another process's folder; the
    /// session learns it from the shell's OSC 7 or OSC 9;9 reports.
    pub const fn working_directory(&self) -> Option<PathBuf> {
        None
    }

    /// An identity for what runs in front: the shell when idle, a recognized
    /// agent's process, or a number for the current run of other programs.
    pub fn foreground_process_group(&self) -> Option<u32> {
        if self.opaque {
            return Some(self.child_pid);
        }
        let observation = self.observe();
        if observation.others.is_empty() {
            return Some(self.child_pid);
        }
        Some(
            observation
                .agent
                .map_or(u32::MAX - observation.period, |(pid, _)| pid),
        )
    }

    pub fn foreground_is_child(&self) -> bool {
        !self.opaque && self.observe().others.is_empty()
    }

    pub fn foreground_program(&self) -> Option<String> {
        if self.opaque {
            return None;
        }
        self.observe().agent.map(|(_, name)| name.to_owned())
    }

    /// All input travels through `PtyReader::transfer` on Windows.
    pub fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        Ok(0)
    }

    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        _width_pixels: Option<u16>,
        _height_pixels: Option<u16>,
    ) -> Result<()> {
        self.console.as_ref().map_or(Ok(()), |console| {
            // SAFETY: the console is open.
            Ok(hresult(unsafe {
                (console.api.resize)(console.handle, size(rows, cols))
            })?)
        })
    }

    pub fn request_shutdown(&mut self, stage: ShutdownStage) -> Result<()> {
        match stage {
            ShutdownStage::Interrupt => {
                drop(self.input.send(Write {
                    bytes: vec![0x03],
                    done: None,
                }));
            }
            ShutdownStage::Hangup => drop(self.console.take()),
            ShutdownStage::Terminate | ShutdownStage::Force => {
                // SAFETY: the job handle is open.
                if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
        }
        Ok(())
    }

    pub async fn wait(&mut self) -> Result<Option<i32>> {
        loop {
            if let Some(code) = self.exit_code()? {
                return Ok(Some(code));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    pub async fn wait_within(&mut self, grace: Duration) -> Result<WaitOutcome> {
        tokio::time::timeout(grace, async {
            let code = self.wait().await?;
            while !self.job_processes().is_empty() {
                tokio::time::sleep(POLL).await;
            }
            Ok(code)
        })
        .await
        .map_or(Ok(WaitOutcome::TimedOut), |result| {
            result.map(WaitOutcome::Reaped)
        })
    }

    fn exit_code(&self) -> Result<Option<i32>> {
        // SAFETY: the process handle is open.
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
            return Ok(None);
        }
        let mut code = 0u32;
        // SAFETY: the process handle is open and `code` is a valid output.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &raw mut code) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Some(code.cast_signed()))
    }

    fn job_processes(&self) -> Vec<u32> {
        let header = std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
            / size_of::<usize>();
        let mut capacity = 32;
        loop {
            let mut buffer = vec![0usize; header + capacity];
            let bytes = u32::try_from(size_of_val(buffer.as_slice())).unwrap_or(u32::MAX);
            // SAFETY: the buffer is aligned for the list and `bytes` long.
            let succeeded = unsafe {
                QueryInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    bytes,
                    ptr::null_mut(),
                )
            } != 0;
            let more = !succeeded
                && io::Error::last_os_error().raw_os_error() == Some(ERROR_MORE_DATA.cast_signed());
            if !succeeded && !more {
                return Vec::new();
            }
            // SAFETY: the buffer starts with the list header.
            let list = unsafe { &*buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
            let assigned = list.NumberOfAssignedProcesses as usize;
            let listed = (list.NumberOfProcessIdsInList as usize).min(capacity);
            if (more || assigned > listed) && capacity < MAX_JOB_PROCESSES {
                capacity = (capacity * 2).min(MAX_JOB_PROCESSES);
                continue;
            }
            return buffer[header..header + listed]
                .iter()
                .filter_map(|pid| u32::try_from(*pid).ok())
                .collect();
        }
    }

    fn observe(&self) -> Observation {
        let processes = self.job_processes();
        let others: Vec<u32> = processes
            .iter()
            .copied()
            .filter(|pid| *pid != self.child_pid)
            .collect();
        let mut observed = self.observed.lock().unwrap_or_else(PoisonError::into_inner);
        observed.identities.retain(|pid, _| processes.contains(pid));
        // A terminal started as an agent, as a resumed conversation is, runs
        // that agent in front of whatever it starts, as on Unix.
        let root = processes
            .contains(&self.child_pid)
            .then_some(self.child_pid);
        let mut agent = None;
        for pid in root.into_iter().chain(others.iter().copied()) {
            let identity = *observed
                .identities
                .entry(pid)
                .or_insert_with(|| image_identity(pid));
            if agent.is_none() {
                agent = identity.map(|name| (pid, name));
            }
        }
        let busy = !others.is_empty();
        if busy && !observed.busy {
            observed.period = observed.period.wrapping_add(1);
        }
        observed.busy = busy;
        Observation {
            others,
            agent,
            period: observed.period,
        }
    }
}

impl Drop for KodosiPty {
    fn drop(&mut self) {
        // SAFETY: the job handle is open. Closing it would also end the processes.
        unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) };
    }
}

#[derive(Debug)]
pub struct PtyReader {
    output: mpsc::Receiver<io::Result<Vec<u8>>>,
    chunk: Vec<u8>,
    offset: usize,
    input: std_mpsc::Sender<Write>,
    in_flight: Option<oneshot::Receiver<io::Result<usize>>>,
}

enum Event {
    Output(Option<io::Result<Vec<u8>>>),
    Written(std::result::Result<io::Result<usize>, oneshot::error::RecvError>),
}

impl PtyReader {
    /// Whether input handed to the console has not yet been confirmed. The
    /// session keeps that input queued until `transfer` reports it written.
    pub const fn write_in_flight(&self) -> bool {
        self.in_flight.is_some()
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.offset < self.chunk.len() {
            return Ok(self.copy(buf));
        }
        let chunk = self.output.recv().await;
        self.receive(chunk, buf)
    }

    /// Waits for output or, while `write` is given, for the console to take
    /// input. Cancel-safe: a write handed over before cancellation is still
    /// reported by a later call.
    pub async fn transfer(
        &mut self,
        read: &mut [u8],
        write: Option<&[u8]>,
    ) -> io::Result<Transfer> {
        if self.in_flight.is_none()
            && let Some(bytes) = write.filter(|bytes| !bytes.is_empty())
        {
            let (done, completion) = oneshot::channel();
            if self
                .input
                .send(Write {
                    bytes: bytes.to_vec(),
                    done: Some(done),
                })
                .is_ok()
            {
                self.in_flight = Some(completion);
            }
        }
        if self.offset < self.chunk.len() {
            return Ok(Transfer::Read(self.copy(read)));
        }
        let event = {
            let output = &mut self.output;
            let in_flight = &mut self.in_flight;
            tokio::select! {
                biased;
                written = async {
                    match in_flight.as_mut() {
                        Some(completion) => completion.await,
                        None => std::future::pending().await,
                    }
                }, if in_flight.is_some() => Event::Written(written),
                chunk = output.recv() => Event::Output(chunk),
            }
        };
        match event {
            Event::Written(written) => {
                self.in_flight = None;
                match written {
                    Ok(Ok(count)) => Ok(Transfer::Wrote(count)),
                    // The console closed its input, so the session is ending.
                    Ok(Err(_)) | Err(_) => Ok(Transfer::Read(0)),
                }
            }
            Event::Output(chunk) => self.receive(chunk, read).map(Transfer::Read),
        }
    }

    fn receive(&mut self, chunk: Option<io::Result<Vec<u8>>>, buf: &mut [u8]) -> io::Result<usize> {
        match chunk {
            Some(Ok(chunk)) => {
                self.chunk = chunk;
                self.offset = 0;
                Ok(self.copy(buf))
            }
            Some(Err(error)) => Err(error),
            None => Ok(0),
        }
    }

    fn copy(&mut self, buf: &mut [u8]) -> usize {
        let count = buf.len().min(self.chunk.len() - self.offset);
        buf[..count].copy_from_slice(&self.chunk[self.offset..self.offset + count]);
        self.offset += count;
        count
    }
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = ptr::null_mut();
    let mut write: HANDLE = ptr::null_mut();
    // SAFETY: both outputs are valid; the handles are not inheritable.
    if unsafe { CreatePipe(&raw mut read, &raw mut write, ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `CreatePipe` succeeded, so both handles are owned.
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

fn create_job() -> io::Result<OwnedHandle> {
    // SAFETY: an unnamed job with default security.
    let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `CreateJobObjectW` returned an owned handle.
    let job = unsafe { OwnedHandle::from_raw_handle(job) };
    // SAFETY: all-zero limits are valid before the flag is set.
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: the job is open and `limits` matches the information class.
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            u32::try_from(size_of_val(&limits)).unwrap_or(u32::MAX),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(job)
}

struct AttributeList(LPPROC_THREAD_ATTRIBUTE_LIST);

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: the list was initialized and is deleted once.
        unsafe { DeleteProcThreadAttributeList(self.0) };
    }
}

fn start(
    console: &Console,
    job: &OwnedHandle,
    application: &Path,
    line: &mut [u16],
    environment: &[u16],
    directory: Option<&Path>,
) -> Result<(OwnedHandle, u32)> {
    let mut bytes = 0;
    // SAFETY: a null list asks for the required size.
    unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &raw mut bytes) };
    let mut storage = vec![0u64; bytes.div_ceil(8)];
    let list: LPPROC_THREAD_ATTRIBUTE_LIST = storage.as_mut_ptr().cast();
    // SAFETY: the storage holds `bytes` aligned bytes.
    if unsafe { InitializeProcThreadAttributeList(list, 2, 0, &raw mut bytes) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let list = AttributeList(list);
    let job_handle = job.as_raw_handle();
    // SAFETY: the pseudoconsole attribute takes the handle value itself; the job
    // list takes a pointer to one handle that outlives process creation.
    if unsafe {
        UpdateProcThreadAttribute(
            list.0,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            console.handle as *const c_void,
            size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null(),
        )
    } == 0
        || unsafe {
            UpdateProcThreadAttribute(
                list.0,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                (&raw const job_handle).cast(),
                size_of::<HANDLE>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: all-zero startup information is valid before the fields below are set.
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = u32::try_from(size_of::<STARTUPINFOEXW>()).unwrap_or(u32::MAX);
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.lpAttributeList = list.0;
    // SAFETY: all-zero process information is a valid output.
    let mut information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let application = wide(application.as_os_str());
    let directory = directory.map(|directory| wide(directory.as_os_str()));
    // A program inherits its parent's choice to ignore Ctrl+C, which a host
    // started in a new process group makes, so Ctrl+C is restored first.
    // SAFETY: a null handler only changes this process's Ctrl+C attribute.
    unsafe { SetConsoleCtrlHandler(None, 0) };
    // SAFETY: every string is NUL-terminated, the command line is writable, and
    // the environment block is double-NUL-terminated UTF-16.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            directory.as_ref().map_or(ptr::null(), Vec::as_ptr),
            &raw const startup.StartupInfo,
            &raw mut information,
        )
    } == 0
    {
        return Err(KodosiError::Spawn(format!(
            "could not start the program: {}",
            io::Error::last_os_error()
        )));
    }
    // SAFETY: `CreateProcessW` succeeded, so both handles are owned.
    let (process, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(information.hProcess),
            OwnedHandle::from_raw_handle(information.hThread),
        )
    };
    drop(thread);
    Ok((process, information.dwProcessId))
}

fn spawn_writer(pipe: OwnedHandle, requests: std_mpsc::Receiver<Write>) -> io::Result<()> {
    std::thread::Builder::new()
        .name("kodosi-pty-input".to_owned())
        .stack_size(THREAD_STACK)
        .spawn(move || {
            for request in requests {
                let result = write_all(&pipe, &request.bytes).map(|()| request.bytes.len());
                let failed = result.is_err();
                if let Some(done) = request.done {
                    drop(done.send(result));
                }
                if failed {
                    break;
                }
            }
        })?;
    Ok(())
}

fn write_all(pipe: &OwnedHandle, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let mut written = 0u32;
        let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        // SAFETY: the pipe is open and the buffer holds `length` bytes.
        if unsafe {
            WriteFile(
                pipe.as_raw_handle(),
                bytes.as_ptr(),
                length,
                &raw mut written,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
}

fn spawn_reader(pipe: OwnedHandle) -> io::Result<mpsc::Receiver<io::Result<Vec<u8>>>> {
    let (sender, receiver) = mpsc::channel(8);
    std::thread::Builder::new()
        .name("kodosi-pty-output".to_owned())
        .stack_size(THREAD_STACK)
        .spawn(move || {
            let mut buffer = vec![0u8; READ_BYTES];
            loop {
                let mut count = 0u32;
                // SAFETY: the pipe is open and the buffer holds `READ_BYTES` bytes.
                let succeeded = unsafe {
                    ReadFile(
                        pipe.as_raw_handle(),
                        buffer.as_mut_ptr(),
                        u32::try_from(buffer.len()).unwrap_or(u32::MAX),
                        &raw mut count,
                        ptr::null_mut(),
                    )
                } != 0;
                let item = if succeeded {
                    if count == 0 {
                        continue;
                    }
                    Ok(buffer[..count as usize].to_vec())
                } else {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(ERROR_BROKEN_PIPE.cast_signed()) {
                        Ok(Vec::new())
                    } else {
                        Err(error)
                    }
                };
                let last = !matches!(&item, Ok(bytes) if !bytes.is_empty());
                if sender.blocking_send(item).is_err() || last {
                    break;
                }
            }
        })?;
    Ok(receiver)
}

fn resolve_program(program: &Path, working_dir: Option<&Path>) -> Result<PathBuf> {
    let candidate = if program.is_absolute() {
        program.to_owned()
    } else if program.components().count() > 1 {
        working_dir.map_or_else(|| program.to_owned(), |directory| directory.join(program))
    } else {
        program
            .to_str()
            .and_then(kodosi_os::process::find_executable)
            .unwrap_or_default()
    };
    // App execution aliases are reparse points that only `symlink_metadata` reads.
    if std::fs::symlink_metadata(&candidate).is_ok_and(|metadata| !metadata.is_dir()) {
        Ok(candidate)
    } else {
        Err(KodosiError::Spawn(format!(
            "command not found: {}",
            program.display()
        )))
    }
}

fn is_system_bash(program: &Path) -> bool {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return false;
    };
    program
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("bash"))
        && program.parent().is_some_and(|parent| {
            parent
                .as_os_str()
                .eq_ignore_ascii_case(Path::new(&root).join("System32"))
        })
}

fn is_batch(program: &Path) -> bool {
    program.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
    })
}

/// The application to start and its command line. Batch files run through
/// `cmd.exe /d /s /c`, which only admits arguments that `cmd` cannot reinterpret.
fn command_line(program: &Path, arguments: &[String]) -> Result<(PathBuf, Vec<u16>)> {
    let mut line = Vec::new();
    if !is_batch(program) {
        push_argument(&mut line, program.as_os_str());
        for argument in arguments {
            line.push(u16::from(b' '));
            push_argument(&mut line, OsStr::new(argument));
        }
        line.push(0);
        return Ok((program.to_owned(), line));
    }
    let script = program
        .to_str()
        .filter(|script| safe_for_cmd(script) && !script.contains('"'));
    let Some(script) = script else {
        return Err(KodosiError::Spawn(format!(
            "the script path cannot run safely through cmd.exe: {}",
            program.display()
        )));
    };
    if let Some(argument) = arguments.iter().find(|argument| !safe_for_cmd(argument)) {
        return Err(KodosiError::Spawn(format!(
            "the argument cannot pass safely through cmd.exe: {argument}"
        )));
    }
    let shell = std::env::var_os("ComSpec").map_or_else(
        || {
            std::env::var_os("SystemRoot")
                .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
                .join(r"System32\cmd.exe")
        },
        PathBuf::from,
    );
    let mut command = format!("\"{script}\"");
    for argument in arguments {
        if argument.contains(' ') {
            command.push_str(" \"");
            command.push_str(argument);
            command.push('"');
        } else {
            command.push(' ');
            command.push_str(argument);
        }
    }
    push_argument(&mut line, shell.as_os_str());
    line.extend(format!(" /d /s /c \"{command}\"").encode_utf16());
    line.push(0);
    Ok((shell, line))
}

fn safe_for_cmd(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_alphanumeric() || " _-=.:/\\@+,#~[]{}$'".contains(character)
        })
}

/// Appends one argument quoted as `CommandLineToArgvW` reads it back.
fn push_argument(line: &mut Vec<u16>, argument: &OsStr) {
    let units: Vec<u16> = argument.encode_wide().collect();
    let space = u16::from(b' ');
    let tab = u16::from(b'\t');
    let quote = u16::from(b'"');
    let backslash = u16::from(b'\\');
    if !units.is_empty() && !units.iter().any(|unit| [space, tab, quote].contains(unit)) {
        line.extend(units);
        return;
    }
    line.push(quote);
    let mut backslashes = 0;
    for unit in units {
        if unit == backslash {
            backslashes += 1;
        } else {
            if unit == quote {
                line.extend(std::iter::repeat_n(backslash, backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    line.extend(std::iter::repeat_n(backslash, backslashes));
    line.push(quote);
}

/// The inherited environment without other terminals' and agents' context,
/// plus the terminal's own variables, sorted as Windows expects.
fn environment_block(overrides: &[(&str, &str)]) -> Vec<u16> {
    let mut variables: BTreeMap<String, (OsString, OsString)> = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.is_empty() || inherits_foreign_context(&name) {
            continue;
        }
        variables.insert(name.to_uppercase(), (key.clone(), value));
    }
    for (key, value) in [
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
        ("TERM_PROGRAM", "Kodosi"),
    ]
    .into_iter()
    .chain(overrides.iter().copied())
    {
        variables.insert(key.to_uppercase(), (key.into(), value.into()));
    }
    let mut block = Vec::new();
    for (key, value) in variables.values() {
        block.extend(key.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    if block.len() == 1 {
        block.push(0);
    }
    block
}

fn image_identity(pid: u32) -> Option<&'static str> {
    // SAFETY: `OpenProcess` has no pointer arguments.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: `OpenProcess` returned an owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut buffer = [0u16; 1024];
    let mut length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: the buffer holds `length` UTF-16 units.
    if unsafe {
        QueryFullProcessImageNameW(
            handle.as_raw_handle(),
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    } == 0
    {
        return None;
    }
    let path = PathBuf::from(OsString::from_wide(buffer.get(..length as usize)?));
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    program_identity(&[stem])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(line: &[u16]) -> String {
        String::from_utf16_lossy(&line[..line.len() - 1])
    }

    #[test]
    fn arguments_are_quoted_as_windows_reads_them() {
        let mut line = Vec::new();
        for (argument, expected) in [
            ("plain", "plain"),
            ("two words", "\"two words\""),
            ("", "\"\""),
            (r#"say "hi""#, r#""say \"hi\"""#),
            (r"C:\path with space\", r#""C:\path with space\\""#),
        ] {
            line.clear();
            push_argument(&mut line, OsStr::new(argument));
            assert_eq!(String::from_utf16_lossy(&line), expected);
        }
    }

    #[test]
    fn batch_launchers_run_through_cmd_with_safe_arguments_only() {
        let (application, line) = command_line(
            Path::new(r"C:\npm\claude.cmd"),
            &["--resume".into(), "abc-1".into()],
        )
        .unwrap();
        assert!(
            application
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("cmd.exe")
        );
        assert!(arguments(&line).ends_with(r#" /d /s /c ""C:\npm\claude.cmd" --resume abc-1""#));
        for unsafe_argument in ["a&b", "%PATH%", "a|b", "a\"b", "!x!", "a^b", "a<b", ""] {
            assert!(
                command_line(Path::new(r"C:\npm\claude.cmd"), &[unsafe_argument.into()]).is_err()
            );
        }
        let (application, line) =
            command_line(Path::new(r"C:\Program Files\PowerShell\7\pwsh.exe"), &[]).unwrap();
        assert_eq!(
            application,
            Path::new(r"C:\Program Files\PowerShell\7\pwsh.exe")
        );
        assert_eq!(
            arguments(&line),
            r#""C:\Program Files\PowerShell\7\pwsh.exe""#
        );
    }

    #[test]
    fn bare_names_come_from_path_and_never_from_the_folder() {
        let folder = tempfile::tempdir().unwrap();
        let shadow = folder.path().join("cmd.exe");
        std::fs::write(&shadow, b"").unwrap();
        let resolved = resolve_program(Path::new("cmd"), Some(folder.path())).unwrap();
        assert_ne!(resolved, shadow);
        assert!(
            resolved
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("cmd.exe"))
        );
        assert!(resolve_program(Path::new("kodosi-missing-program"), None).is_err());
    }

    #[test]
    fn a_terminal_started_as_an_agent_reports_that_agent() {
        let folder = tempfile::tempdir().unwrap();
        let agent = folder.path().join("codex.exe");
        let system = std::env::var_os("SystemRoot").unwrap();
        std::fs::copy(Path::new(&system).join(r"System32\cmd.exe"), &agent).unwrap();
        let (pty, _reader) =
            KodosiPty::spawn_program(&agent, &["/d".into(), "/k".into()], None, 24, 80, &[])
                .unwrap();
        assert_eq!(pty.foreground_program().as_deref(), Some("codex"));
    }

    #[test]
    fn environment_drops_foreign_context_and_sets_the_terminal() {
        let block = environment_block(&[("KODOSI_SESSION_ID", "id")]);
        let text = String::from_utf16_lossy(&block);
        let variables: Vec<&str> = text.split('\0').filter(|entry| !entry.is_empty()).collect();
        assert!(variables.contains(&"TERM=xterm-256color"));
        assert!(variables.contains(&"TERM_PROGRAM=Kodosi"));
        assert!(variables.contains(&"KODOSI_SESSION_ID=id"));
        assert!(
            !variables
                .iter()
                .any(|entry| entry.starts_with("WT_SESSION="))
        );
        assert!(block.ends_with(&[0, 0]));
    }
}
