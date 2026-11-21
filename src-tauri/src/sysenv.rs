//! Process helpers: environment cleanup for child programs, PATH lookup, opening files/URLs.

use std::{
    env,
    ffi::OsString,
    path::PathBuf,
    process::{Command, Stdio},
};

/// Variables an AppImage runtime injects that break unrelated programs (browsers, terminals, editors).
const APPIMAGE_VARS: [&str; 16] = [
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "GSETTINGS_SCHEMA_DIR",
    "GTK_DATA_PREFIX",
    "GTK_EXE_PREFIX",
    "GTK_PATH",
    "GTK_THEME",
    "GTK_IM_MODULE_FILE",
    "GDK_BACKEND",
    "GDK_PIXBUF_MODULE_FILE",
    "GIO_EXTRA_MODULES",
    "GI_TYPELIB_PATH",
    "PYTHONHOME",
    "PYTHONPATH",
    "PERL5LIB",
    "QT_PLUGIN_PATH",
];

/// Strip AppImage-specific variables so child programs use the system libraries.
pub fn clean_env<C: CommandEnv>(cmd: &mut C) {
    let Some(appdir) = env::var_os("APPDIR").filter(|v| !v.is_empty()) else {
        return;
    };
    for var in APPIMAGE_VARS {
        cmd.remove(var);
    }
    if let Some(dirs) = env::var_os("XDG_DATA_DIRS") {
        let appdir = PathBuf::from(appdir);
        let kept: Vec<PathBuf> = env::split_paths(&dirs).filter(|p| !p.starts_with(&appdir)).collect();
        match env::join_paths(kept) {
            Ok(v) if !v.is_empty() => cmd.set("XDG_DATA_DIRS", v),
            _ => cmd.remove("XDG_DATA_DIRS"),
        }
    }
}

pub trait CommandEnv {
    fn remove(&mut self, key: &str);
    fn set(&mut self, key: &str, value: OsString);
}

impl CommandEnv for Command {
    fn remove(&mut self, key: &str) {
        self.env_remove(key);
    }
    fn set(&mut self, key: &str, value: OsString) {
        self.env(key, value);
    }
}

impl CommandEnv for tokio::process::Command {
    fn remove(&mut self, key: &str) {
        self.env_remove(key);
    }
    fn set(&mut self, key: &str, value: OsString) {
        self.env(key, value);
    }
}

/// Locate `name` on PATH (plus the usual Unix bin folders), honouring PATHEXT on Windows.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("PATH").map(|p| env::split_paths(&p).collect()).unwrap_or_default();
    if cfg!(unix) {
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".local/bin"));
        }
        dirs.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    }
    let exts: Vec<String> = if cfg!(windows) {
        env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        vec![String::new()]
    };
    dirs.iter()
        .filter(|d| !d.as_os_str().is_empty())
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("{name}{e}"))))
        .find(|p| p.is_file())
}

/// Open a file or http(s) URL with the user's default application, without AppImage env leaks.
pub fn open_detached(target: impl AsRef<std::ffi::OsStr>) -> std::io::Result<()> {
    let mut last = std::io::Error::other("no opener available");
    for mut cmd in open::commands(target.as_ref()) {
        clean_env(&mut cmd);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        match cmd.spawn() {
            Ok(mut child) => {
                // Reap in the background so no zombie stays behind.
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
            Err(err) => last = err,
        }
    }
    Err(last)
}

/// Path that re-launches this program (the AppImage itself when running from one).
pub fn self_exe() -> std::io::Result<PathBuf> {
    if let Some(appimage) = env::var_os("APPIMAGE").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(appimage);
        if path.is_file() {
            return Ok(path);
        }
    }
    env::current_exe()
}
