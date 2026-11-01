//! Bounded managed subprocesses with retained logs and whole-tree cancellation.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static CANCELLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct Limits {
    pub timeout: Duration,
    pub log_bytes: u64,
    pub processes: u32,
    pub memory_bytes: usize,
}
impl Limits {
    pub fn environment() -> Result<Self> {
        fn number(key: &str, default: u64) -> Result<u64> {
            std::env::var(key).map_or(Ok(default), |s| {
                s.parse().with_context(|| format!("Invalid {key}"))
            })
        }
        Ok(Self {
            timeout: Duration::from_secs(number("CPUV_PROCESS_TIMEOUT_SECS", 3600)?),
            log_bytes: number("CPUV_MAX_LOG_MIB", 64)?
                .checked_mul(1024 * 1024)
                .context("Log size overflow")?,
            processes: number("CPUV_MAX_PROCESSES", 256)?.try_into()?,
            memory_bytes: number("CPUV_MAX_PROCESS_MEMORY_MIB", 0)?
                .checked_mul(1024 * 1024)
                .context("Memory limit overflow")?
                .try_into()?,
        })
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    use windows_sys::Win32::{
        Foundation::*,
        System::{
            Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler},
            Diagnostics::ToolHelp::*,
            JobObjects::*,
            Threading::*,
        },
    };
    pub struct Job(HANDLE);
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    unsafe extern "system" fn control(event: u32) -> i32 {
        if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
            CANCELLED.store(true, Ordering::SeqCst);
            1
        } else {
            0
        }
    }
    pub fn configure(command: &mut Command, limits: &Limits) -> Result<Job> {
        static HANDLER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        ensure!(
            *HANDLER.get_or_init(|| unsafe { SetConsoleCtrlHandler(Some(control), 1) != 0 }),
            "Cannot install build cancellation handler"
        );
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        ensure!(
            !handle.is_null(),
            "Cannot create build job: {}",
            std::io::Error::last_os_error()
        );
        let job = Job(handle);
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if limits.processes > 0 {
            information.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            information.BasicLimitInformation.ActiveProcessLimit = limits.processes;
        }
        if limits.memory_bytes > 0 {
            information.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            information.JobMemoryLimit = limits.memory_bytes;
        }
        ensure!(
            unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &information as *const _ as _,
                    std::mem::size_of_val(&information) as u32,
                )
            } != 0,
            "Cannot set build resource limits: {}",
            std::io::Error::last_os_error()
        );
        // Assign the process to its job before any application code can spawn children.
        command.creation_flags(CREATE_SUSPENDED);
        Ok(job)
    }
    pub fn resume(job: &Job, child: &std::process::Child) -> Result<()> {
        ensure!(
            unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as _) } != 0,
            "Cannot attach managed process to its job: {}",
            std::io::Error::last_os_error()
        );
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        ensure!(
            snapshot != INVALID_HANDLE_VALUE,
            "Cannot enumerate suspended process thread"
        );
        let snapshot = Job(snapshot);
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut present = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
        let mut resumed = false;
        while present {
            if entry.th32OwnerProcessID == child.id() {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                ensure!(!thread.is_null(), "Cannot open suspended process thread");
                let thread = Job(thread);
                ensure!(
                    unsafe { ResumeThread(thread.0) } != u32::MAX,
                    "Cannot resume managed process"
                );
                resumed = true;
            }
            present = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
        }
        ensure!(resumed, "Managed process has no resumable primary thread");
        Ok(())
    }
    pub fn terminate(job: &Job) {
        unsafe {
            TerminateJobObject(job.0, 1);
        }
    }
}

pub fn run(command: &mut Command, log: &Path, limits: &Limits) -> Result<ExitStatus> {
    ensure!(
        limits.log_bytes > 0,
        "CPUV_MAX_LOG_MIB must be greater than zero"
    );
    fs::create_dir_all(log.parent().context("Build log has no parent")?)?;
    let mut file = OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "command: {command:?}")?;
    let initial = file.metadata()?.len();
    ensure!(
        initial < limits.log_bytes,
        "Build log already exceeds the configured limit: {}",
        log.display()
    );
    let file = Arc::new(Mutex::new(file));
    let bytes = Arc::new(AtomicU64::new(initial));
    let overflow = Arc::new(AtomicBool::new(false));
    CANCELLED.store(false, Ordering::SeqCst);
    #[cfg(windows)]
    let job = windows::configure(command, limits)?;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Starting managed build command")?;
    #[cfg(windows)]
    if let Err(error) = windows::resume(&job, &child) {
        windows::terminate(&job);
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    fn stream(
        mut input: impl Read + Send + 'static,
        stderr: bool,
        file: Arc<Mutex<std::fs::File>>,
        bytes: Arc<AtomicU64>,
        overflow: Arc<AtomicBool>,
        cap: u64,
    ) -> std::thread::JoinHandle<Result<()>> {
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    return Ok(());
                }
                let previous = bytes.fetch_add(count as u64, Ordering::SeqCst);
                if previous.saturating_add(count as u64) > cap {
                    overflow.store(true, Ordering::SeqCst);
                    return Ok(());
                }
                file.lock()
                    .map_err(|_| anyhow::anyhow!("Build log lock failed"))?
                    .write_all(&buffer[..count])?;
                if stderr {
                    std::io::stderr().write_all(&buffer[..count])?;
                } else {
                    std::io::stdout().write_all(&buffer[..count])?;
                }
            }
        })
    }
    let output = stream(
        child.stdout.take().unwrap(),
        false,
        file.clone(),
        bytes.clone(),
        overflow.clone(),
        limits.log_bytes,
    );
    let errors = stream(
        child.stderr.take().unwrap(),
        true,
        file.clone(),
        bytes,
        overflow.clone(),
        limits.log_bytes,
    );
    let started = Instant::now();
    let mut reason = None;
    let status = loop {
        if CANCELLED.load(Ordering::SeqCst) {
            reason = Some("cancelled");
        }
        if !limits.timeout.is_zero() && started.elapsed() >= limits.timeout {
            reason = Some("time limit exceeded");
        }
        if overflow.load(Ordering::SeqCst) {
            reason = Some("log size limit exceeded");
        }
        if reason.is_some() {
            #[cfg(windows)]
            windows::terminate(&job);
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    // Job close also reaps descendants that outlive a successful parent.
    #[cfg(windows)]
    drop(job);
    let first = output
        .join()
        .map_err(|_| anyhow::anyhow!("Build stdout reader failed"))?;
    let second = errors
        .join()
        .map_err(|_| anyhow::anyhow!("Build stderr reader failed"))?;
    if overflow.load(Ordering::SeqCst) {
        reason = Some("log size limit exceeded");
    }
    if let Some(reason) = reason {
        anyhow::bail!(
            "Managed command {reason}; process tree stopped. Log: {}",
            log.display()
        );
    }
    first?;
    second?;
    file.lock()
        .map_err(|_| anyhow::anyhow!("Build log lock failed"))?
        .sync_all()?;
    Ok(status)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn timeout_stops_child_tree_and_retains_diagnostics() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("timeout.log");
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/c", "echo retained & ping -n 30 127.0.0.1"]);
        let limits = Limits {
            timeout: Duration::from_millis(150),
            log_bytes: 65536,
            processes: 16,
            memory_bytes: 0,
        };
        let start = Instant::now();
        assert!(
            run(&mut command, &log, &limits)
                .unwrap_err()
                .to_string()
                .contains("time limit")
        );
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(fs::read_to_string(log).unwrap().contains("retained"));
    }
}
