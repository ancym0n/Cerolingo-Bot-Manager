//! Minimal Chrome DevTools Protocol client: everything the bots need from Playwright,
//! without Node, Python or a driver process. One browser = one page = one CDP session.

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::Message;

use super::ui;
use crate::{paths, sysenv};

const CMD_TIMEOUT: Duration = Duration::from_secs(20);

// -- errors -------------------------------------------------------------------------------

/// The page / browser is gone (closed by the user, crashed, or the connection dropped).
#[derive(Debug)]
pub struct WindowClosed;

impl fmt::Display for WindowClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("The browser window was closed")
    }
}
impl std::error::Error for WindowClosed {}

/// A bot step failed; `done` is how many lessons were finished before it.
#[derive(Debug)]
pub struct StepError {
    pub msg: String,
    pub done: usize,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}
impl std::error::Error for StepError {}

pub fn step_error(msg: impl Into<String>) -> anyhow::Error {
    StepError { msg: msg.into(), done: 0 }.into()
}

pub fn is_window_closed(err: &anyhow::Error) -> bool {
    err.chain().any(|e| e.is::<WindowClosed>())
}

// -- connection ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Event {
    pub method: String,
    pub session: Option<String>,
}

type Reply = std::result::Result<Value, String>;

struct Conn {
    out: mpsc::UnboundedSender<String>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Reply>>>,
    next_id: AtomicU64,
    events: broadcast::Sender<Event>,
    closed: AtomicBool,
    target: OnceLock<String>,
    session: OnceLock<String>,
}

impl Conn {
    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn post(&self, id: u64, method: &str, params: Value, session: Option<&str>) -> bool {
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = Value::from(s);
        }
        self.out.send(msg.to_string()).is_ok()
    }

    async fn send(&self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        if self.closed() {
            return Err(WindowClosed.into());
        }
        let (tx, rx) = oneshot::channel();
        let id = self.new_id();
        // Register before sending so a fast reply can never be missed.
        self.pending.lock().unwrap().insert(id, tx);
        if !self.post(id, method, params, session) {
            self.pending.lock().unwrap().remove(&id);
            return Err(WindowClosed.into());
        }
        match tokio::time::timeout(CMD_TIMEOUT, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(msg))) if !self.closed() => Err(anyhow!("{method}: {msg}")),
            Ok(_) => Err(WindowClosed.into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                if self.closed() { Err(WindowClosed.into()) } else { bail!("{method} timed out") }
            }
        }
    }
}

async fn reader<S>(conn: Arc<Conn>, mut read: S)
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = read.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };
        let Ok(mut v) = serde_json::from_str::<Value>(text.as_str()) else { continue };
        if let Some(id) = v.get("id").and_then(Value::as_u64) {
            let waiter = conn.pending.lock().unwrap().remove(&id);
            if let Some(tx) = waiter {
                let reply = match v.get("error") {
                    Some(e) => Err(e["message"].as_str().unwrap_or("CDP error").to_owned()),
                    None => Ok(v["result"].take()),
                };
                let _ = tx.send(reply);
            }
            continue;
        }
        let Some(method) = v["method"].as_str().map(str::to_owned) else { continue };
        let params = v["params"].take();
        let session = v["sessionId"].as_str().map(str::to_owned);
        let ours = session.is_some() && session.as_ref() == conn.session.get();
        match method.as_str() {
            "Target.targetDestroyed" | "Target.targetCrashed"
                if params["targetId"].as_str() == conn.target.get().map(String::as_str) =>
            {
                conn.closed.store(true, Ordering::Release);
            }
            "Target.detachedFromTarget"
                if params["sessionId"].as_str() == conn.session.get().map(String::as_str) =>
            {
                conn.closed.store(true, Ordering::Release);
            }
            "Inspector.targetCrashed" | "Inspector.detached" if ours => conn.closed.store(true, Ordering::Release),
            // alert()/confirm()/beforeunload would freeze the page: accept them like Playwright does.
            "Page.javascriptDialogOpening" if ours => {
                conn.post(conn.new_id(), "Page.handleJavaScriptDialog", json!({ "accept": true }), session.as_deref());
            }
            _ => {}
        }
        let _ = conn.events.send(Event { method, session });
    }
    conn.closed.store(true, Ordering::Release);
    // Dropping the senders wakes every waiter with WindowClosed.
    conn.pending.lock().unwrap().clear();
}

// -- browser ------------------------------------------------------------------------------

pub struct Options<'a> {
    pub size: (u32, u32),
    pub mute: bool,
    pub headless: bool,
    pub init_script: Option<&'a str>,
}

/// A throw-away Chromium profile shown as a clean app window (no tabs / address bar).
pub struct Browser {
    conn: Arc<Conn>,
    session: String,
    target: String,
    child: Option<tokio::process::Child>,
    profile: Option<PathBuf>,
    tasks: Vec<JoinHandle<()>>,
}

fn remove_dir_retry(path: &Path) {
    for _ in 0..20 {
        match std::fs::remove_dir_all(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => std::thread::sleep(Duration::from_millis(150)),
            _ => return,
        }
    }
}

fn chrome_args(profile: &Path, opts: &Options) -> Vec<String> {
    let mut args: Vec<String> = [
        "--remote-debugging-port=0",
        "--disable-blink-features=AutomationControlled",
        "--force-dark-mode",
        "--no-first-run",
        "--no-default-browser-check",
        "--password-store=basic",
        "--use-mock-keychain",
        "--lang=en-US",
        "--accept-lang=en-US,en",
        "--disable-features=Translate,PasswordLeakDetection,OptimizationHints,MediaRouter,\
         DialMediaRouteProvider,GlobalMediaControls,LensOverlay,AutofillServerCommunication",
        "--disable-background-networking",
        "--disable-background-timer-throttling",
        "--disable-backgrounding-occluded-windows",
        "--disable-renderer-backgrounding",
        "--disable-breakpad",
        "--disable-component-update",
        "--disable-default-apps",
        "--disable-dev-shm-usage",
        "--disable-extensions",
        "--disable-hang-monitor",
        "--disable-prompt-on-repost",
        "--disable-search-engine-choice-screen",
        "--disable-sync",
        "--metrics-recording-only",
        "--no-service-autorun",
        "--force-color-profile=srgb",
    ]
    .map(String::from)
    .into();
    args.push(format!("--user-data-dir={}", profile.display()));
    args.push(format!("--window-size={},{}", opts.size.0, opts.size.1));
    if cfg!(target_os = "linux") {
        args.push("--no-sandbox".into());
    }
    if opts.mute {
        args.push("--mute-audio".into());
    }
    if opts.headless {
        args.push("--headless=new".into());
        args.push("--hide-scrollbars".into());
        args.push("about:blank".into());
    } else {
        args.push("--app=about:blank".into());
    }
    args
}

async fn wait_port_file(profile: &Path, child: &mut tokio::process::Child) -> Result<String> {
    let port_file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = tokio::fs::read_to_string(&port_file).await {
            let mut lines = text.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next())
                && let Ok(port) = port.trim().parse::<u16>()
                && path.trim().starts_with("/devtools/")
            {
                return Ok(format!("ws://127.0.0.1:{port}{}", path.trim()));
            }
        }
        if let Some(status) = child.try_wait()? {
            bail!("Chromium exited during startup ({status})");
        }
        if Instant::now() > deadline {
            bail!("Chromium did not start within 30 s");
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

impl Browser {
    pub async fn launch(exe: &Path, opts: &Options<'_>) -> Result<Browser> {
        let profile = tempfile::Builder::new()
            .prefix("cbm-profile-")
            .tempdir()
            .context("cannot create a browser profile")?
            .keep();
        let mut cmd = tokio::process::Command::new(exe);
        cmd.args(chrome_args(&profile, opts))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        sysenv::clean_env(&mut cmd);
        #[cfg(target_os = "linux")]
        // SAFETY: prctl is async-signal-safe; makes Chromium die with the bot process.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                remove_dir_retry(&profile);
                return Err(anyhow!("cannot start {}: {e}", exe.display()));
            }
        };
        let mut browser = Browser {
            conn: Arc::new(Conn {
                out: mpsc::unbounded_channel().0,
                pending: Mutex::default(),
                next_id: AtomicU64::new(0),
                events: broadcast::channel(1).0,
                closed: AtomicBool::new(true),
                target: OnceLock::new(),
                session: OnceLock::new(),
            }),
            session: String::new(),
            target: String::new(),
            child: Some(child),
            profile: Some(profile),
            tasks: Vec::new(),
        };
        // On error `browser` is dropped here, which kills Chromium and removes the profile.
        browser.connect(opts).await?;
        Ok(browser)
    }

    async fn connect(&mut self, opts: &Options<'_>) -> Result<()> {
        let profile = self.profile.clone().expect("profile");
        let ws_url = wait_port_file(&profile, self.child.as_mut().expect("child")).await?;
        let (stream, _) = tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async(&ws_url))
            .await
            .map_err(|_| anyhow!("timed out connecting to Chromium"))?
            .context("cannot connect to Chromium")?;
        let (mut write, read) = stream.split();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        let conn = Arc::new(Conn {
            out: out_tx,
            pending: Mutex::default(),
            next_id: AtomicU64::new(0),
            events: broadcast::channel(2048).0,
            closed: AtomicBool::new(false),
            target: OnceLock::new(),
            session: OnceLock::new(),
        });
        self.tasks.push(tokio::spawn(async move {
            while let Some(text) = out_rx.recv().await {
                if write.send(Message::text(text)).await.is_err() {
                    break;
                }
            }
            let _ = write.close().await;
        }));
        self.tasks.push(tokio::spawn(reader(conn.clone(), read)));
        self.conn = conn;

        self.conn.send("Target.setDiscoverTargets", json!({ "discover": true }), None).await?;
        let target = self.find_page_target().await?;
        let attached = self
            .conn
            .send("Target.attachToTarget", json!({ "targetId": target, "flatten": true }), None)
            .await?;
        let session = attached["sessionId"].as_str().ok_or_else(|| anyhow!("no CDP session"))?.to_owned();
        let _ = self.conn.target.set(target.clone());
        let _ = self.conn.session.set(session.clone());
        self.target = target;
        self.session = session;

        self.cmd("Page.enable", json!({})).await?;
        let dark = json!({ "features": [{ "name": "prefers-color-scheme", "value": "dark" }] });
        let _ = self.cmd("Emulation.setEmulatedMedia", dark).await;
        let _ = self.cmd("Emulation.setLocaleOverride", json!({ "locale": "en-US" })).await;
        if opts.headless {
            let version = self.conn.send("Browser.getVersion", json!({}), None).await?;
            if let Some(ua) = version["userAgent"].as_str() {
                let ua = ua.replace("HeadlessChrome", "Chrome");
                let params = json!({ "userAgent": ua, "acceptLanguage": "en-US,en" });
                let _ = self.cmd("Emulation.setUserAgentOverride", params).await;
            }
        }
        if let Some(script) = opts.init_script {
            self.cmd("Page.addScriptToEvaluateOnNewDocument", json!({ "source": script })).await?;
        }
        Ok(())
    }

    async fn find_page_target(&self) -> Result<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let list = self.conn.send("Target.getTargets", json!({}), None).await?;
            let found = list["targetInfos"].as_array().and_then(|infos| {
                infos.iter().find(|t| {
                    t["type"] == "page" && !t["url"].as_str().unwrap_or("").starts_with("devtools://")
                })
            });
            if let Some(id) = found.and_then(|t| t["targetId"].as_str()) {
                return Ok(id.to_owned());
            }
            if Instant::now() > deadline {
                let created = self.conn.send("Target.createTarget", json!({ "url": "about:blank" }), None).await?;
                return created["targetId"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no page target"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn is_closed(&self) -> bool {
        self.conn.closed()
    }

    pub fn ensure_open(&self) -> Result<()> {
        if self.is_closed() { Err(WindowClosed.into()) } else { Ok(()) }
    }

    /// Page-level CDP command.
    pub async fn cmd(&self, method: &str, params: Value) -> Result<Value> {
        self.conn.send(method, params, Some(&self.session)).await
    }

    pub async fn url(&self) -> String {
        self.conn
            .send("Target.getTargetInfo", json!({ "targetId": self.target }), None)
            .await
            .ok()
            .and_then(|v| v["targetInfo"]["url"].as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    pub async fn eval(&self, expression: &str) -> Result<Value> {
        let params = json!({
            "expression": expression, "returnByValue": true, "awaitPromise": true, "userGesture": true,
        });
        let mut res = self.cmd("Runtime.evaluate", params).await?;
        if let Some(ex) = res.get("exceptionDetails") {
            let text = ex["exception"]["description"].as_str().or(ex["text"].as_str()).unwrap_or("error");
            bail!("JavaScript error: {}", text.lines().next().unwrap_or(text));
        }
        Ok(res["result"]["value"].take())
    }

    /// `(func)(...args)` in the page; `func` is a JS function expression.
    pub async fn call(&self, func: &str, args: &[Value]) -> Result<Value> {
        self.eval(&format!("({func})(...{})", Value::Array(args.to_vec()))).await
    }

    /// Evaluate `body` (an expression) with the element helpers in scope; errors become `null`.
    async fn with_helpers(&self, body: &str) -> Value {
        self.eval(&format!("(async () => {{ {HELPERS}\nreturn ({body}); }})()")).await.unwrap_or(Value::Null)
    }

    pub async fn eval_bool(&self, func: &str) -> bool {
        self.call(func, &[]).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
    }

    // -- element helpers (first *visible* match, like Playwright's `visible=true`) --------

    pub async fn visible(&self, selector: &str) -> bool {
        self.with_helpers(&format!("!!__find({})", js(selector))).await == Value::Bool(true)
    }

    pub async fn visible_text(&self, selector: &str, pattern: &str) -> bool {
        self.with_helpers(&format!("!!__find({}, [{}, 'i'])", js(selector), js(pattern))).await == Value::Bool(true)
    }

    pub async fn click(&self, selector: &str) -> bool {
        self.click_js(&format!("__find({})", js(selector)), 1.5).await
    }

    pub async fn click_text(&self, selector: &str, pattern: &str) -> bool {
        self.click_js(&format!("__find({}, [{}, 'i'])", js(selector), js(pattern)), 1.5).await
    }

    /// Real mouse click when the element is reachable, DOM click when something (e.g. the mod
    /// menu) covers it. `find` is a JS expression returning the element (helpers in scope).
    pub async fn click_js(&self, find: &str, wait_secs: f64) -> bool {
        let script = format!(
            r#"(async () => {{ {HELPERS}
            const end = Date.now() + {wait_ms};
            let el = {find};
            while (!el && Date.now() < end) {{ await new Promise(r => setTimeout(r, 100)); el = {find}; }}
            if (!el) return null;
            if (el.scrollIntoViewIfNeeded) el.scrollIntoViewIfNeeded(true);
            else el.scrollIntoView({{ block: 'center', inline: 'center' }});
            const frame = () => new Promise(r => {{ let d = false; const f = () => {{ if (!d) {{ d = true; r(); }} }};
                requestAnimationFrame(f); setTimeout(f, 50); }});
            let prev = '';
            for (let i = 0; i < 8; i++) {{
                await frame();
                const b = el.getBoundingClientRect();
                const key = `${{b.left}}|${{b.top}}|${{b.width}}|${{b.height}}`;
                if (key === prev) break;
                prev = key;
            }}
            if (!el.isConnected) return null;
            const b = el.getBoundingClientRect();
            const x = b.left + b.width / 2, y = b.top + b.height / 2;
            const top = document.elementFromPoint(x, y);
            if (!top || !(el === top || el.contains(top))) {{ el.click(); return 'dom'; }}
            window.__cbmClickTarget = el;
            return {{ x, y }};
        }})()"#,
            wait_ms = (wait_secs * 1000.0) as u64,
        );
        let res = match self.eval(&script).await {
            Ok(v) => v,
            Err(_) => return false,
        };
        let (Some(x), Some(y)) = (res["x"].as_f64(), res["y"].as_f64()) else {
            return !res.is_null();
        };
        if self.mouse_click(x, y).await.is_err() {
            let _ = self.eval("window.__cbmClickTarget && window.__cbmClickTarget.click()").await;
        }
        true
    }

    async fn mouse_click(&self, x: f64, y: f64) -> Result<()> {
        let event = |kind: &str, buttons: u8| {
            json!({ "type": kind, "x": x, "y": y, "button": if kind == "mouseMoved" { "none" } else { "left" },
                    "buttons": buttons, "clickCount": 1 })
        };
        self.cmd("Input.dispatchMouseEvent", event("mouseMoved", 0)).await?;
        self.cmd("Input.dispatchMouseEvent", event("mousePressed", 1)).await?;
        self.cmd("Input.dispatchMouseEvent", event("mouseReleased", 0)).await?;
        Ok(())
    }

    pub async fn wait_enabled(&self, selector: &str, timeout: f64) -> bool {
        let check = format!(
            "(sel => {{ const el = [...document.querySelectorAll(sel)].find(e => e.getClientRects().length);
                return Boolean(el && !el.disabled && el.getAttribute('aria-disabled') !== 'true'); }})({})",
            js(selector)
        );
        wait_until(timeout, 0.08, async || self.eval(&check).await.ok() == Some(Value::Bool(true))).await
    }

    /// Type `value` into the first visible `selector` (React-safe; retries char by char).
    pub async fn fill(&self, selector: &str, value: &str) -> Result<()> {
        let find = format!("__find({})", js(selector));
        if !self.click_js(&find, 3.0).await {
            self.ensure_open()?;
            bail!("Element not found: {selector}");
        }
        let select = format!(
            "(() => {{ const el = {find}; if (!el) return false; el.focus();
                try {{ el.select(); }} catch (_) {{}} document.execCommand('selectAll'); return true; }})()"
        );
        let read = format!("(() => {{ const el = {find}; return el ? String(el.value ?? '') : null; }})()");
        self.with_helpers(&select).await;
        self.cmd("Input.insertText", json!({ "text": value })).await?;
        if self.with_helpers(&read).await.as_str() != Some(value) {
            // Controlled inputs occasionally drop the first write: clear and type slowly.
            self.with_helpers(&select).await;
            self.with_helpers("document.execCommand('delete')").await;
            for ch in value.chars() {
                self.cmd("Input.insertText", json!({ "text": ch.to_string() })).await?;
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        }
        Ok(())
    }

    pub async fn dismiss_overlays(&self) {
        let _ = self.call(DISMISS_JS, &[]).await;
    }

    async fn navigate(&self, url: &str) -> Result<()> {
        let mut events = self.conn.events.subscribe();
        let res = self.cmd("Page.navigate", json!({ "url": url })).await?;
        if let Some(err) = res["errorText"].as_str().filter(|e| !e.is_empty()) {
            bail!("{err} at {url}");
        }
        if res["loaderId"].as_str().is_none_or(str::is_empty) {
            return Ok(()); // same-document navigation
        }
        let wait = async {
            loop {
                // Short timeout so a dead browser is noticed quickly (no more events arrive).
                let Ok(next) = tokio::time::timeout(Duration::from_millis(250), events.recv()).await else {
                    if self.is_closed() {
                        return Err(WindowClosed.into());
                    }
                    continue;
                };
                match next {
                    Ok(ev) if ev.method == "Page.domContentEventFired" && ev.session.as_deref() == Some(&self.session) => {
                        return Ok(());
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Err(anyhow::Error::from(WindowClosed)),
                }
                if self.is_closed() {
                    return Err(WindowClosed.into());
                }
            }
        };
        match tokio::time::timeout(Duration::from_secs(25), wait).await {
            Ok(r) => r,
            Err(_) => {
                self.ensure_open()?;
                let state = self.eval("document.readyState").await.unwrap_or(Value::Null);
                if state.as_str().is_some_and(|s| s != "loading") {
                    Ok(())
                } else {
                    bail!("Navigation to {url} timed out")
                }
            }
        }
    }

    pub async fn goto(&self, url: &str) -> Result<()> {
        let leaf = url.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_owned();
        let retries = 3;
        for attempt in 0..retries {
            self.ensure_open()?;
            match self.navigate(url).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    self.ensure_open()?;
                    if attempt == retries - 1 {
                        return Err(err);
                    }
                    let msg = format!("{err:#}").to_lowercase();
                    if (msg.contains("interrupted") || msg.contains("aborted")) && self.url().await.contains(&leaf) {
                        return Ok(()); // the SPA redirected mid-navigation, we are where we wanted to be
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
        unreachable!()
    }

    pub async fn screenshot(&self, path: &Path) -> Result<()> {
        use base64::Engine;
        let res = self.cmd("Page.captureScreenshot", json!({ "format": "png" })).await?;
        let data = base64::engine::general_purpose::STANDARD.decode(res["data"].as_str().unwrap_or(""))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, data)?;
        Ok(())
    }

    pub async fn debug_dump(&self, name: &str) {
        let shot = paths::logs_dir().join(format!("{name}.png"));
        if self.screenshot(&shot).await.is_ok() {
            ui::dim(&format!("url: {}", self.url().await));
            ui::dim(&format!("screenshot: {}", shot.display()));
        }
    }

    pub async fn cookies(&self) -> Vec<Value> {
        match self.conn.send("Storage.getCookies", json!({}), None).await {
            Ok(mut v) => v["cookies"].as_array_mut().map(std::mem::take).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    pub async fn set_cookies(&self, cookies: Vec<Value>) -> Result<()> {
        self.conn.send("Storage.setCookies", json!({ "cookies": cookies }), None).await?;
        Ok(())
    }

    pub async fn jwt_cookie(&self) -> Option<String> {
        self.cookies()
            .await
            .into_iter()
            .find(|c| c["name"] == "jwt_token")
            .and_then(|c| c["value"].as_str().filter(|v| !v.is_empty()).map(str::to_owned))
    }

    /// True once the window holds a jwt_token cookie and Duolingo shows the home page.
    pub async fn logged_in(&self) -> bool {
        self.jwt_cookie().await.is_some() && self.url().await.contains("/learn")
    }

    pub async fn wait_closed(&self) {
        while !self.is_closed() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn close(mut self) {
        if !self.is_closed() {
            let _ = tokio::time::timeout(Duration::from_secs(3), self.conn.send("Browser.close", json!({}), None)).await;
        }
        if let Some(mut child) = self.child.take()
            && tokio::time::timeout(Duration::from_secs(5), child.wait()).await.is_err()
        {
            let _ = child.kill().await;
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(profile) = self.profile.take() {
            let _ = tokio::task::spawn_blocking(move || remove_dir_retry(&profile)).await;
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let end = Instant::now() + Duration::from_secs(3);
            while !matches!(child.try_wait(), Ok(Some(_)) | Err(_)) && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(30));
            }
        }
        if let Some(profile) = self.profile.take() {
            remove_dir_retry(&profile);
        }
    }
}

// -- shared helpers -----------------------------------------------------------------------

/// JSON-encode a value as a JS literal.
pub fn js<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

/// Poll `f` every `interval` seconds until it returns true or `timeout` seconds pass.
pub async fn wait_until(timeout: f64, interval: f64, mut f: impl AsyncFnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs_f64(timeout);
    loop {
        if f().await {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        tokio::time::sleep(Duration::from_secs_f64(interval)).await;
    }
}

pub async fn sleep(secs: f64) {
    tokio::time::sleep(Duration::from_secs_f64(secs)).await;
}

const HELPERS: &str = r#"
const __v = (el) => { if (!el || !el.isConnected) return false; const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden'; };
const __text = (el) => (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim();
const __find = (sel, re) => { const rx = re ? new RegExp(re[0], re[1] || '') : null;
    for (const el of document.querySelectorAll(sel)) if (__v(el) && (!rx || rx.test(__text(el)))) return el;
    return null; };
"#;

pub const DISMISS_JS: &str = r#"() => {
    const ok = el => el && !el.disabled && el.getAttribute('aria-disabled') !== 'true' && el.getClientRects().length;
    for (const sel of ['#onetrust-accept-btn-handler', '#onetrust-reject-all-handler',
                       '[data-test="notification-drawer-no-thanks-button"]', '[data-test="plus-no-thanks"]']) {
        const el = document.querySelector(sel);
        if (ok(el)) { el.click(); return sel; }
    }
    const texts = new Set(['accept all', 'no thanks', 'maybe later', 'not now', "i'll do it later"]);
    for (const b of document.querySelectorAll('button')) {
        if (!ok(b) || b.closest('#cerolingoModMenu, [data-test^="challenge"]')) continue;
        if (texts.has((b.innerText || '').trim().toLowerCase())) { b.click(); return b.innerText; }
    }
    return null;
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a real browser: `CBM_CHROME=/usr/bin/chromium cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn drives_real_chrome() {
        let exe = PathBuf::from(std::env::var("CBM_CHROME").expect("CBM_CHROME"));
        let init = "window.__cbm = 41 + 1;";
        let opts = Options { size: (800, 600), mute: true, headless: true, init_script: Some(init) };
        let b = Browser::launch(&exe, &opts).await.unwrap();
        b.goto("data:text/html,<input id=n><button onclick=\"this.textContent='done'\">Go</button>").await.unwrap();
        assert_eq!(b.eval("window.__cbm").await.unwrap(), Value::from(42));
        b.fill("#n", "zażółć \"x\"").await.unwrap();
        assert_eq!(b.eval("document.querySelector('#n').value").await.unwrap(), Value::from("zażółć \"x\""));
        assert!(b.click_text("button", "^Go$").await);
        assert!(wait_until(3.0, 0.05, async || b.visible_text("button", "^done$").await).await);
        assert!(!b.is_closed());
        b.close().await;
    }
}
