//! The browser harness (ADR-0042, plans/c4 step 3): a native test builds a
//! wasm page, serves it from `127.0.0.1` with or without the isolation
//! headers (COOP `same-origin`, COEP `require-corp`), opens it in headless
//! Chromium and waits for the page to POST its report to `/report`. The
//! report is whatever text the page sends, JSON by convention.
//!
//! Without Chromium a browser test skips loudly ([`chromium`]), as the
//! visual tests skip without lavapipe; with `ARRIX_REQUIRE_BROWSER` set, as
//! CI sets it, a missing browser fails instead. `ARRIX_CHROMIUM` names the
//! binary when it is not on `PATH` as `chromium`, `chromium-browser`,
//! `google-chrome` or `google-chrome-stable`.
//!
//! Native only, and never built for wasm32 (`scripts/wasm-lint`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;
use std::{env, fs, thread};

/// The browsers looked for on `PATH`, in order.
const BROWSERS: [&str; 4] = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
];

/// The workspace root.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The workspace's target directory, where pages are built and the
/// browser's profiles live.
pub fn target_dir() -> PathBuf {
    // A relative one is the workspace's, not the running test's crate's.
    workspace_root().join(env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target".into()))
}

/// The Chromium to run, or `None` after saying loudly that `test` skips.
///
/// # Panics
///
/// With `ARRIX_REQUIRE_BROWSER` set and no browser found.
pub fn chromium(test: &str) -> Option<PathBuf> {
    let found = env::var_os("ARRIX_CHROMIUM")
        .map(PathBuf::from)
        .or_else(|| {
            let path = env::var_os("PATH")?;
            env::split_paths(&path)
                .flat_map(|dir| BROWSERS.iter().map(move |b| dir.join(b)))
                .find(|p| p.is_file())
        });
    if found.is_none() {
        let required = env::var("ARRIX_REQUIRE_BROWSER")
            .is_ok_and(|v| !matches!(v.as_str(), "" | "0" | "false"));
        assert!(
            !required,
            "{test}: no Chromium found ({BROWSERS:?} on PATH, or ARRIX_CHROMIUM), and ARRIX_REQUIRE_BROWSER is set"
        );
        // To the process's stderr, not through `eprintln!`, which the test
        // runner captures for a passing test and would make the skip silent.
        let _ = writeln!(
            std::io::stderr(),
            "SKIPPING {test}: no Chromium found ({BROWSERS:?} on PATH, or ARRIX_CHROMIUM). \
             Install Chromium to run the browser tests."
        );
    }
    found
}

/// Builds `package`'s example `example` for wasm32 and runs `wasm-bindgen
/// --target web` on it into `out`, as `<example>.js` and
/// `<example>_bg.wasm`.
///
/// # Panics
///
/// When either command fails.
pub fn build_example(package: &str, example: &str, out: &Path) {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    run(Command::new(cargo)
        .current_dir(workspace_root())
        .args(["build", "--target", "wasm32-unknown-unknown", "-p", package])
        .args(["--example", example]));
    let wasm = target_dir()
        .join("wasm32-unknown-unknown/debug/examples")
        .join(format!("{example}.wasm"));
    run(Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(out)
        .arg(&wasm));
}

/// Builds `package`'s library for wasm32 as a cdylib (`cargo rustc --lib
/// --crate-type cdylib`, so its native builds link none) and runs
/// `wasm-bindgen --target web` on it into `out`, as `<crate>.js` and
/// `<crate>_bg.wasm`: `scripts/web-build`'s recipe.
///
/// # Panics
///
/// When either command fails.
pub fn build_lib(package: &str, out: &Path) {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    run(Command::new(cargo)
        .current_dir(workspace_root())
        .args(["rustc", "--target", "wasm32-unknown-unknown", "-p", package])
        .args(["--lib", "--crate-type", "cdylib"]));
    let wasm = target_dir()
        .join("wasm32-unknown-unknown/debug")
        .join(format!("{}.wasm", package.replace('-', "_")));
    run(Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(out)
        .arg(&wasm));
}

fn run(command: &mut Command) {
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("{command:?} did not start: {e}"));
    assert!(status.success(), "{command:?} failed: {status}");
}

/// Copies every file of `from` into `to`, flat.
///
/// # Panics
///
/// On an I/O error.
pub fn copy_files(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
        }
    }
}

/// Serves `dir` (with the isolation headers when `isolated`), opens `page`
/// in headless `browser`, and returns the body of the page's first POST to
/// `/report`, then closes the browser and the server.
///
/// # Panics
///
/// When no report comes within `timeout`, with the browser's output.
pub fn open(browser: &Path, dir: &Path, page: &str, isolated: bool, timeout: Duration) -> String {
    let server = Server::start(dir.to_owned(), isolated);
    let profile = profile_dir();
    let log = profile.with_extension("log");
    let mut chrome = Command::new(browser);
    chrome
        .args([
            "--headless",
            "--no-sandbox",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-gpu",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("http://{}/{page}", server.addr))
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut chrome, 0);
    let mut chrome = chrome
        .spawn()
        .unwrap_or_else(|e| panic!("{} did not start: {e}", browser.display()));
    let report = server.reports.recv_timeout(timeout);
    close(&mut chrome);
    server.stop();
    let _ = fs::remove_dir_all(&profile);
    report.unwrap_or_else(|_| {
        let output = fs::read_to_string(&log).unwrap_or_default();
        panic!("{page}: no report within {timeout:?}; the browser said:\n{output}")
    })
}

/// A fresh browser profile under the target directory: a snap Chromium
/// cannot write the host's `/tmp`.
fn profile_dir() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = target_dir()
        .join("web-test")
        .join(format!("profile-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Ends the browser and, on Unix, every process it started (its own
/// group).
fn close(chrome: &mut Child) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-TERM", "--", &format!("-{}", chrome.id())])
        .status();
    #[cfg(not(unix))]
    let _ = chrome.kill();
    let _ = chrome.wait();
}

struct Server {
    addr: SocketAddr,
    reports: mpsc::Receiver<String>,
    stopped: Arc<AtomicBool>,
}

impl Server {
    fn start(dir: PathBuf, isolated: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, reports) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let (dir, tx) = (dir.clone(), tx.clone());
                thread::spawn(move || {
                    if let Ok(stream) = stream {
                        let _ = serve(stream, &dir, isolated, &tx);
                    }
                });
            }
        });
        Self {
            addr,
            reports,
            stopped,
        }
    }

    /// Ends the accept loop: the flag, then one connection to wake it.
    fn stop(self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.addr);
    }
}

/// Answers one request: a file of `dir` for a GET, the report for a POST
/// to `/report`.
fn serve(
    stream: TcpStream,
    dir: &Path,
    isolated: bool,
    reports: &Sender<String>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let path = path.split('?').next().unwrap_or("/");
    let (status, kind, body) = match method {
        "POST" if path == "/report" => {
            let mut body = vec![0; length];
            reader.read_exact(&mut body)?;
            let _ = reports.send(String::from_utf8_lossy(&body).into_owned());
            ("204 No Content", "text/plain", Vec::new())
        }
        "GET" => match file(dir, path) {
            Some((kind, body)) => ("200 OK", kind, body),
            None => ("404 Not Found", "text/plain", b"not found".to_vec()),
        },
        _ => ("405 Method Not Allowed", "text/plain", Vec::new()),
    };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    if isolated {
        head.push_str("Cross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Embedder-Policy: require-corp\r\n");
    }
    head.push_str("\r\n");
    let mut stream = stream;
    stream.write_all(head.as_bytes())?;
    stream.write_all(&body)
}

/// A file of `dir` by its URL path, with its media type; never outside
/// `dir`.
fn file(dir: &Path, path: &str) -> Option<(&'static str, Vec<u8>)> {
    let relative = path.trim_start_matches('/');
    if relative.split('/').any(|part| part == "..") {
        return None;
    }
    let path = dir.join(relative);
    let kind = match path.extension()?.to_str()? {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    Some((kind, fs::read(path).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_never_leaves_the_served_directory() {
        let dir = workspace_root().join("crates");
        assert!(file(&dir, "/arrix-web-test/Cargo.toml").is_some());
        assert!(file(&dir, "/../Cargo.toml").is_none());
        assert!(file(&dir, "/arrix-web-test/../../Cargo.toml").is_none());
    }
}
