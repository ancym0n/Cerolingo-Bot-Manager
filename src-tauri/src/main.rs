// Release builds are GUI-subsystem on Windows; bot mode attaches/allocates its own console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod bot;
mod chromium;
mod gui;
mod net;
mod paths;
mod sysenv;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("bot") {
        std::process::exit(bot::main(&args[1..]));
    }
    paths::migrate_legacy_data();
    gui::run();
}
