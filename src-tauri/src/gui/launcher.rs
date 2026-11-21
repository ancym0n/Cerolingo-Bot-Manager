//! Start a bot as a separate process: in a terminal window when the console is wanted,
//! otherwise silently in the background.

use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};

use crate::sysenv;

fn reap(mut child: std::process::Child) {
    std::thread::spawn(move || child.wait());
}

fn hidden(exe: &Path, args: &[String]) -> io::Result<()> {
    let mut cmd = Command::new(exe);
    cmd.arg("bot").args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    sysenv::clean_env(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    reap(cmd.spawn()?);
    Ok(())
}

pub fn spawn_bot(args: &[String], console: bool) -> io::Result<()> {
    // The AppImage path, not the mount: the mount disappears when the GUI exits.
    let exe = sysenv::self_exe()?;
    if !console {
        return hidden(&exe, args);
    }
    platform::console(&exe, args)
}

#[cfg(any(unix, test))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::process::CommandExt;

    pub fn console(exe: &Path, args: &[String]) -> io::Result<()> {
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let mut cmd = Command::new(exe);
        cmd.args(["bot", "--console", "--pause"]).args(args).creation_flags(CREATE_NEW_CONSOLE);
        reap(cmd.spawn()?);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn console(exe: &Path, args: &[String]) -> io::Result<()> {
        let line = std::iter::once(exe.to_string_lossy().into_owned())
            .chain(["bot".into(), "--pause".into()])
            .chain(args.iter().cloned())
            .map(|a| sh_quote(&a))
            .collect::<Vec<_>>()
            .join(" ");
        let escaped = line.replace('\\', "\\\\").replace('"', "\\\"");
        let mut cmd = Command::new("osascript");
        cmd.arg("-e").arg(format!("tell application \"Terminal\" to do script \"{escaped}\""));
        cmd.arg("-e").arg("tell application \"Terminal\" to activate");
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        reap(cmd.spawn()?);
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::*;

    /// Terminal emulators and the arguments that precede the command to run.
    const TERMINALS: [(&str, &[&str]); 10] = [
        ("x-terminal-emulator", &["-e"]),
        ("ghostty", &["-e"]),
        ("alacritty", &["-e"]),
        ("kitty", &[]),
        ("foot", &[]),
        ("wezterm", &["start", "--"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("xterm", &["-e"]),
    ];

    pub fn console(exe: &Path, args: &[String]) -> io::Result<()> {
        let command = std::iter::once(exe.to_string_lossy().into_owned())
            .chain(["bot".into()])
            .chain(args.iter().cloned())
            .map(|a| sh_quote(&a))
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "printf '\\033]0;Cerolingo Bot Manager\\007'; {command}; code=$?; \
             printf '\\n[Process finished with exit code %s]\\nPress Enter to close...' \"$code\"; read _"
        );
        let preferred = std::env::var("TERMINAL").ok().filter(|t| !t.is_empty());
        let candidates = preferred
            .iter()
            .map(|t| (t.as_str(), &["-e"][..]))
            .chain(TERMINALS.iter().map(|(n, a)| (*n, *a)));
        for (name, prefix) in candidates {
            let Some(path) = sysenv::find_executable(name) else { continue };
            let mut cmd = Command::new(path);
            cmd.args(prefix).args(["sh", "-c"]).arg(&script);
            cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            sysenv::clean_env(&mut cmd);
            if let Ok(child) = cmd.spawn() {
                reap(child);
                return Ok(());
            }
        }
        // No terminal emulator: still run the bot, just without a visible console.
        hidden(exe, args)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn quotes_for_sh() {
        assert_eq!(super::sh_quote("a b"), "'a b'");
        assert_eq!(super::sh_quote("it's"), r"'it'\''s'");
    }
}
