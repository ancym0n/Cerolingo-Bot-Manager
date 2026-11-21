//! Console output helpers: one consistent, readable look for every bot.

use std::{
    io::{IsTerminal, Write},
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use crate::paths::{APP_NAME, APP_VERSION};

const WIDTH: usize = 58;

static COLOR: LazyLock<bool> = LazyLock::new(|| {
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let force = std::env::var_os("FORCE_COLOR").is_some_and(|v| !v.is_empty());
    (std::io::stdout().is_terminal() || force) && !no_color
});
static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
static INLINE: AtomicBool = AtomicBool::new(false);

fn rgb(r: u8, g: u8, b: u8) -> String {
    if *COLOR { format!("\x1b[38;2;{r};{g};{b}m") } else { String::new() }
}
fn code(s: &'static str) -> &'static str {
    if *COLOR { s } else { "" }
}
fn reset() -> &'static str {
    code("\x1b[0m")
}
fn bold() -> &'static str {
    code("\x1b[1m")
}
fn primary() -> String {
    rgb(104, 119, 254)
}
fn light() -> String {
    rgb(179, 208, 255)
}
fn green() -> String {
    rgb(123, 255, 208)
}
fn red() -> String {
    rgb(255, 133, 133)
}
fn yellow() -> String {
    rgb(255, 214, 120)
}
fn gray() -> String {
    rgb(130, 142, 160)
}

/// Write to stdout ignoring errors (a closed console must never crash a bot).
fn out(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

pub fn init() {
    LazyLock::force(&STARTED);
}

/// Finish a pending in-place progress line before printing anything else.
fn brk() {
    if INLINE.swap(false, Ordering::Relaxed) {
        out("\n");
    }
}

fn stamp() -> String {
    format!("{}{}{}", gray(), chrono::Local::now().format("%H:%M:%S"), reset())
}

fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width { text.to_owned() } else { format!("{text}{}", " ".repeat(width - len)) }
}

fn line(tag: &str, color: String, text: &str) {
    brk();
    out(&format!("{} {color}{}{tag:<5}{} {text}\n", stamp(), bold(), reset()));
}

pub fn set_title(text: &str) {
    if *COLOR {
        out(&format!("\x1b]0;{text}\x07"));
    }
}

pub fn banner(title: &str, subtitle: &str) {
    set_title(&format!("{APP_NAME} - {title}"));
    let (p, r) = (primary(), reset());
    let bar = "─".repeat(WIDTH);
    let head = format!("{APP_NAME}  v{APP_VERSION}");
    let mut s = format!("\n{p}╭{bar}╮{r}\n");
    s += &format!("{p}│{r} {}{}{}{r}{p}│{r}\n", bold(), light(), pad(&head, WIDTH - 1));
    s += &format!("{p}│{r} {}{p}│{r}\n", pad(title, WIDTH - 1));
    if !subtitle.is_empty() {
        s += &format!("{p}│{r} {}{}{r}{p}│{r}\n", gray(), pad(subtitle, WIDTH - 1));
    }
    s += &format!("{p}╰{bar}╯{r}\n\n");
    out(&s);
}

pub fn section(text: &str) {
    brk();
    out(&format!("\n{}{}▌ {text}{}\n", primary(), bold(), reset()));
}

pub fn step(index: usize, total: usize, text: &str) {
    brk();
    let (b, r) = (bold(), reset());
    out(&format!("{} {}{b}[{index}/{total}]{r} {b}{text}{r}\n", stamp(), primary()));
}

pub fn info(text: &str) {
    line("INFO", light(), text);
}
pub fn ok(text: &str) {
    line("OK", green(), text);
}
pub fn warn(text: &str) {
    line("WARN", yellow(), text);
}
pub fn error(text: &str) {
    line("ERROR", red(), text);
}
pub fn dim(text: &str) {
    brk();
    out(&format!("{}       {}{text}{}\n", stamp(), gray(), reset()));
}

pub enum Val {
    Text(String),
    Flag(bool),
}

impl From<&str> for Val {
    fn from(s: &str) -> Self {
        Val::Text(s.to_owned())
    }
}
impl From<String> for Val {
    fn from(s: String) -> Self {
        Val::Text(s)
    }
}
impl From<bool> for Val {
    fn from(b: bool) -> Self {
        Val::Flag(b)
    }
}

fn table(rows: &[(&str, Val)]) {
    let width = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    let mut s = String::new();
    for (key, value) in rows {
        let value = match value {
            Val::Text(t) => t.clone(),
            Val::Flag(true) => format!("{}on{}", green(), reset()),
            Val::Flag(false) => format!("{}off{}", gray(), reset()),
        };
        s += &format!("  {}{}{}  {value}\n", gray(), pad(key, width), reset());
    }
    s.push('\n');
    out(&s);
}

pub fn config(rows: &[(&str, Val)]) {
    section("Configuration");
    table(rows);
}

/// Draw / update a single-line progress bar (call `progress_done` when finished).
pub fn progress(label: &str, done: usize, total: usize, suffix: &str) {
    INLINE.store(true, Ordering::Relaxed);
    let total = total.max(1);
    const CELLS: usize = 24;
    let filled = (CELLS * done / total).min(CELLS);
    if *COLOR {
        let bar = format!("{}{}{}{}{}", primary(), "█".repeat(filled), gray(), "░".repeat(CELLS - filled), reset());
        out(&format!("\r{} {label} {bar} {done}/{total} {suffix}\x1b[K", stamp()));
    } else {
        out(&format!("\r{label} {done}/{total} {suffix}"));
    }
}

pub fn progress_done() {
    brk();
}

pub fn summary(title: &str, rows: &[(&str, Val)]) {
    brk();
    section(title);
    table(rows);
}

pub fn elapsed() -> String {
    let s = STARTED.elapsed().as_secs();
    if s >= 60 { format!("{}m {:02}s", s / 60, s % 60) } else { format!("{s}s") }
}

pub fn farewell(text: &str) {
    brk();
    out(&format!("\n{}{}✔ {text}{} {}({}){}\n", green(), bold(), reset(), gray(), elapsed(), reset()));
}

/// First line of an error chain, for compact console messages.
pub fn first_line(err: &anyhow::Error) -> String {
    let text = format!("{err:#}");
    text.lines().next().unwrap_or("error").to_owned()
}
