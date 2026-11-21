# Cerolingo Bot Manager

![Version](https://img.shields.io/github/v/release/ancym0n/Cerolingo-Bot-Manager?color=6877ff&label=Version&labelColor=e7ecff&style=flat)
[![Website](https://img.shields.io/badge/Website-ancymon.dev-6877ff?labelColor=e7ecff&logo=globe&logoColor=white)](https://ancymon.dev/Cerolingo-Bot-Manager/)

Desktop application for creating and managing Duolingo bot accounts, farming XP, and running the Cerolingo auto-solver. Written in Rust (Tauri) as a single small binary.

![Banner](github/github-banner.png)

## Features

- **Account Creator**: Automatically registers new Duolingo accounts in Chrome/Chromium with customizable username prefixes, email domains, and optional leaderboard qualification (10 lessons).
- **XP Farming**: Farms XP on every saved account in `accounts.txt`. **Legit** mode logs into each account, solves real lessons on duolingo.com/lesson with the Cerolingo mod menu and tracks lessons and XP gained; with Legit off it uses the faster practice-session API.
- **Auto Solver**: Logs in with a saved account (or lets you log in yourself), opens duolingo.com/lesson and solves the number of lessons you choose (0 = until you close the window) using the Cerolingo mod menu, which is downloaded from the [Cerolingo repository](https://github.com/ancym0n/Cerolingo) on every run.
- **Account Manager**: Quickly opens and manages your saved `email:password:cookie` account list.
- **Hidden mode**: Every activity that opens a browser has a **Hidden** toggle to run it in the background.
- **Updates**: Offers new releases from GitHub when they come out (never forced).
- **Cross-Platform**: Runs on both Windows and Linux with a responsive dark interface.

## Installation

### Ready-made builds (recommended)

Download the latest build from the releases page: `CBMv<version>-win.x64-setup.exe` (Windows installer) or `Cerolingo-Bot-Manager-<version>-x86_64.AppImage` (Linux).

Nothing else is needed: no Node.js, Python or Playwright. The bots drive an already installed Chrome, Chromium or Edge directly over the DevTools protocol. If none is found, the app downloads a private Chrome for Testing into its data folder once, showing a progress window. Accounts from older versions are picked up automatically.

### From source

1. Install [Rust](https://rustup.rs/) and the Tauri CLI (`cargo install tauri-cli --version "^2"`). On Linux also install the WebKitGTK dev packages ([list](https://v2.tauri.app/start/prerequisites/#linux)).
2. Run it:
   ```bash
   cd src-tauri
   cargo tauri dev
   ```
3. Build the installers with `cargo tauri build` (output in `src-tauri/target/release/bundle/`).
   Upload the Windows installer as `CBMv<version>-win.x64-setup.exe` so the built-in updater (including the one in older 1.x releases) can find it.

The bots can also be started without the GUI, e.g. `cerolingo-bot-manager bot farm --lessonsPerAccount=5 --legit=false` (`create`, `farm`, `solve`, `update-solver`). `CBM_DATA_DIR` redirects all data and `CBM_CHROME` picks a specific browser.

## Usage

1. Launch the app and click the gear icon next to **Create Account(s)**, **Farm XP**, or **Auto Solver** to configure your settings.
2. Click **Create Account(s)** to generate Duolingo accounts. Saved credentials and cookies are stored automatically in `logs/accounts.txt` inside the app data folder (**Manage Accounts** opens it).
3. Click **Farm XP** to gain XP across all saved accounts, or click **Auto Solver** to open an interactive Duolingo session with the Cerolingo mod menu and auto-solver enabled.
4. Click **Manage Accounts** at any time to view or edit your saved accounts file.

## Cerolingo Extension

Just want the in-page mod menu and auto-solver inside your own browser without the desktop bot manager? Check out the standalone [Cerolingo browser extension](https://ancymon.dev/Cerolingo/) ([GitHub](https://github.com/ancym0n/Cerolingo)) for Chrome, Firefox, and Firefox for Android.
