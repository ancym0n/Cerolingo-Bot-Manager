//! Console bots: `cerolingo-bot-manager bot [--console] [--pause] <command> [--key=value ...]`.

mod cdp;
mod create;
mod farm;
mod lessons;
mod solve;
mod solver;
mod ui;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;

use crate::chromium;

static PAUSE: AtomicBool = AtomicBool::new(false);

/// `--key=value` options after the command name.
pub struct Opts(HashMap<String, String>);

impl Opts {
    fn parse(args: &[String]) -> Opts {
        Opts(
            args.iter()
                .filter_map(|a| a.strip_prefix("--"))
                .map(|a| match a.split_once('=') {
                    Some((k, v)) => (k.to_owned(), v.to_owned()),
                    None => (a.to_owned(), "true".to_owned()),
                })
                .collect(),
        )
    }

    pub fn str(&self, key: &str, default: &str) -> String {
        self.0.get(key).cloned().unwrap_or_else(|| default.to_owned())
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.0.get(key).map_or(default, |v| v.trim().eq_ignore_ascii_case("true"))
    }

    pub fn int(&self, key: &str, default: i64) -> i64 {
        self.0.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
    }
}

const USAGE: &str = "usage: cerolingo-bot-manager bot [--console] [--pause] <create|farm|solve|update-solver> [--key=value ...]";

pub fn main(args: &[String]) -> i32 {
    let mut flags_end = 0;
    let mut console = false;
    for a in args {
        match a.as_str() {
            "--console" => console = true,
            "--pause" => PAUSE.store(true, Ordering::Relaxed),
            _ => break,
        }
        flags_end += 1;
    }
    platform::setup_console(console);
    platform::kill_children_on_exit();
    ui::init();
    std::panic::set_hook(Box::new(|info| {
        ui::error(&format!("Unexpected error: {info}"));
        pause(101);
    }));

    let command = args.get(flags_end).map(String::as_str).unwrap_or("");
    let opts = Opts::parse(&args[(flags_end + 1).min(args.len())..]);
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            ui::error(&format!("Cannot start: {e}"));
            pause(1);
            return 1;
        }
    };
    let code = runtime.block_on(async {
        match command {
            "create" => create::run(&opts).await,
            "farm" => farm::run(&opts).await,
            "solve" => solve::run(&opts).await,
            "update-solver" => solver::update_cli().await,
            _ => {
                ui::error(USAGE);
                2
            }
        }
    });
    // Give browser clean-up tasks a moment, then never hang on exit.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    pause(code);
    code
}

fn pause(code: i32) {
    if !PAUSE.swap(false, Ordering::Relaxed) {
        return;
    }
    use std::io::{BufRead, Write};
    let mut out = std::io::stdout();
    let _ = write!(out, "\n[Process finished with exit code {code}]\nPress Enter to close...");
    let _ = out.flush();
    let _ = std::io::stdin().lock().read_line(&mut String::new());
}

/// Resolve the browser: `--chrome=<path>` from the GUI, otherwise find / download one.
pub async fn browser_exe(opts: &Opts) -> Result<PathBuf> {
    if let Some(p) = opts.0.get("chrome").map(PathBuf::from).filter(|p| p.is_file()) {
        return Ok(p);
    }
    if let Some(p) = chromium::find_installed() {
        return Ok(p);
    }
    ui::info("Downloading Chromium (one-time)...");
    let last = std::sync::Mutex::new(-1i64);
    let exe = chromium::ensure(&|f, _text| {
        let pct = (f * 100.0) as i64;
        let mut last = last.lock().unwrap();
        if pct != *last {
            *last = pct;
            ui::progress("Chromium", pct as usize, 100, "%");
        }
    })
    .await;
    ui::progress_done();
    exe
}

/// Run `work` until it finishes or Ctrl+C is pressed (`None` = interrupted).
pub async fn interruptible<T>(work: impl Future<Output = T>) -> Option<T> {
    tokio::select! {
        r = work => Some(r),
        _ = tokio::signal::ctrl_c() => None,
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        System::{
            Console::{
                ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode,
                GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode,
                SetConsoleOutputCP, SetStdHandle,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
            },
            Threading::GetCurrentProcess,
        },
    };

    fn open(name: &str, access: u32) -> windows_sys::Win32::Foundation::HANDLE {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: valid NUL-terminated wide string, null security attributes / template.
        unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        }
    }

    /// Release builds are GUI programs: attach to the launching console, or open our own one
    /// when `console` is requested. Without `console` a hidden run never shows a window.
    pub fn setup_console(console: bool) {
        // SAFETY: plain Win32 console calls; failures are ignored (no console = silent output).
        unsafe {
            let attached = AttachConsole(ATTACH_PARENT_PROCESS) != 0;
            if !attached && console {
                AllocConsole();
            }
            let out = open("CONOUT$", GENERIC_READ | GENERIC_WRITE);
            if out != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_OUTPUT_HANDLE, out);
                SetStdHandle(STD_ERROR_HANDLE, out);
                let mut mode = 0;
                if GetConsoleMode(out, &mut mode) != 0 {
                    SetConsoleMode(out, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
            let input = open("CONIN$", GENERIC_READ | GENERIC_WRITE);
            if input != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_INPUT_HANDLE, input);
            }
            let _ = GetStdHandle(STD_OUTPUT_HANDLE);
            SetConsoleOutputCP(65001);
        }
    }

    /// Put this process in a job that kills Chromium when the bot exits (even when the console
    /// window is closed with the X button).
    pub fn kill_children_on_exit() {
        // SAFETY: the job handle is intentionally leaked so it lives as long as the process.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok != 0 {
                AssignProcessToJobObject(job, GetCurrentProcess());
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn setup_console(_console: bool) {}
    /// Linux: Chromium gets PR_SET_PDEATHSIG at spawn time (see cdp.rs).
    pub fn kill_children_on_exit() {}
}

#[cfg(test)]
mod tests {
    use super::Opts;

    #[test]
    fn parses_options() {
        let args: Vec<String> = ["--a=1", "--flag", "--b=x=y", "junk", "--n=-3"].map(String::from).into();
        let o = Opts::parse(&args);
        assert_eq!(o.int("a", 0), 1);
        assert!(o.bool("flag", false));
        assert_eq!(o.str("b", ""), "x=y");
        assert_eq!(o.int("n", 0), -3);
        assert_eq!(o.int("missing", 7), 7);
        assert!(!o.bool("a", true));
    }
}
