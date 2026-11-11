//! Getting ComfyUI's program onto this computer. The Windows installer can't hold it (NSIS can't map a
//! 2.5 GB package), so the first start downloads the pinned official portable
//! (`packaging/comfy/version.json`), checks its size and SHA-256 while it arrives, unpacks it with the
//! `7za.exe` shipped beside the host, and keeps it under `<data>/comfy/program/<version>`. A new pin
//! downloads the new version and removes the old one.

use super::comfy;
use crate::LockExt;
use anyhow::{Context, Result, anyhow, bail, ensure};
use futures::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;

/// Free space a download wants: the archive, the unpacked program and room to spare.
const DOWNLOAD_FREE: u64 = 8 * 1024 * 1024 * 1024;
const TRIES: u32 = 4;
/// A connection that sends nothing for this long is dropped and tried again.
const STALL: Duration = Duration::from_mins(1);
const REPORT_EVERY: Duration = Duration::from_millis(500);
/// The folder the official archive unpacks to.
const PORTABLE_ROOT: &str = "ComfyUI_windows_portable";
/// Windows Defender and other scanners keep freshly unpacked files open for a while, and a folder with an
/// open file can't be moved: the move is tried again for about a minute.
const MOVE_TRIES: u32 = 30;
const MOVE_PAUSE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Downloading,
    Unpacking,
    Failed,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Downloading => "downloading",
            Self::Unpacking => "unpacking",
            Self::Failed => "failed",
        }
    }
}

struct Progress {
    phase: Phase,
    done: u64,
    total: u64,
    error: Option<String>,
}

/// What to fetch and how to know it arrived whole.
pub struct Source {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// The archive arrived but isn't the pinned one: trying again would fetch the same bytes.
#[derive(Debug)]
struct Mismatch;

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the download doesn't match the pinned size and SHA-256")
    }
}

impl std::error::Error for Mismatch {}

pub struct Installer {
    progress: Mutex<Progress>,
    /// One install at a time: a second start waits for the first and then finds the program.
    busy: tokio::sync::Mutex<()>,
    events: broadcast::Sender<Value>,
}

impl Installer {
    pub fn new(events: broadcast::Sender<Value>) -> Self {
        let progress = Progress { phase: Phase::Idle, done: 0, total: 0, error: None };
        Self { progress: Mutex::new(progress), busy: tokio::sync::Mutex::new(()), events }
    }

    /// What the window, the card and the phone show while the program arrives.
    pub fn status(&self) -> Value {
        let p = self.progress.locked();
        json!({ "phase": p.phase.name(), "done": p.done, "total": p.total, "error": p.error })
    }

    fn set(&self, phase: Phase, done: u64, total: u64, error: Option<String>) {
        *self.progress.locked() = Progress { phase, done, total, error };
        let _ = self.events.send(json!({ "type": "tool", "id": "comfy" }));
    }

    /// The program's folder, downloading it first when it isn't on this computer yet.
    pub async fn ensure(&self) -> Result<PathBuf> {
        if let Some(dir) = comfy::program_dir() {
            return Ok(dir);
        }
        ensure!(comfy::SUPPORTED, "ComfyUI runs on Windows x64 only for now");
        let _busy = self.busy.lock().await;
        if let Some(dir) = comfy::program_dir() {
            return Ok(dir);
        }
        let result = self.install().await;
        match &result {
            Ok(_) => self.set(Phase::Idle, 0, 0, None),
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "installing ComfyUI");
                self.set(Phase::Failed, 0, 0, Some(format!("{e:#}")));
            }
        }
        result
    }

    async fn install(&self) -> Result<PathBuf> {
        let pin = comfy::pin();
        let seven = seven_zip().context("7za.exe is missing beside the host: reinstall Codync")?;
        let root = comfy::data_dir().join("program");
        tokio::fs::create_dir_all(&root).await.context("creating the ComfyUI program folder")?;
        if let Some(free) = comfy::free_space(&root) {
            ensure!(
                free >= DOWNLOAD_FREE,
                "ComfyUI needs {} GB free to download and unpack; {} GB are free",
                DOWNLOAD_FREE >> 30,
                free >> 30
            );
        }
        let work = root.join(".download");
        tokio::fs::create_dir_all(&work).await.context("preparing the download folder")?;
        let archive = work.join(&pin.asset);
        let source = Source { url: pin.url(), size: pin.size, sha256: pin.sha256.clone() };
        // A start that failed after the download (say, while unpacking) doesn't fetch 2 GB again.
        let (checked, expected) = (archive.clone(), (source.size, source.sha256.clone()));
        let kept = tokio::task::spawn_blocking(move || already_downloaded(&checked, expected.0, &expected.1)).await?;
        if !kept {
            self.set(Phase::Downloading, 0, source.size, None);
            download(&source, &archive, |done| self.set(Phase::Downloading, done, source.size, None)).await?;
        }
        self.set(Phase::Unpacking, 0, 0, None);
        let target = comfy::downloaded_dir();
        let placed = target.clone();
        tokio::task::spawn_blocking(move || unpack_and_place(&seven, &archive, &work, &placed))
            .await
            .context("unpacking ComfyUI")??;
        Ok(target)
    }
}

/// `CODYNC_7ZA` for development, else the `7za.exe` the package puts beside the host.
fn seven_zip() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|p| p.join("7za.exe")));
    std::env::var_os("CODYNC_7ZA").map(PathBuf::from).into_iter().chain(beside).find(|p| p.is_file())
}

/// Downloads `source` into `dest`, checking it as it arrives; `progress` gets the bytes so far.
/// Network failures and server errors are tried again; a wrong archive or a refused address is not.
pub async fn download(source: &Source, dest: &Path, progress: impl Fn(u64)) -> Result<()> {
    let mut attempt = 1;
    loop {
        match fetch_once(source, dest, &progress).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                let _ = tokio::fs::remove_file(dest).await;
                if attempt == TRIES || !retryable(&e) {
                    return Err(e);
                }
                tracing::info!(attempt, error = format!("{e:#}"), "downloading ComfyUI again");
                tokio::time::sleep(Duration::from_secs(5 * u64::from(attempt))).await;
                attempt += 1;
            }
        }
    }
}

fn retryable(e: &anyhow::Error) -> bool {
    if e.downcast_ref::<Mismatch>().is_some() {
        return false;
    }
    let client_error = e
        .chain()
        .filter_map(|cause| cause.downcast_ref::<reqwest::Error>())
        .any(|r| r.status().is_some_and(|s| s.is_client_error()));
    !client_error
}

async fn fetch_once(source: &Source, dest: &Path, progress: &impl Fn(u64)) -> Result<()> {
    let resp = crate::http().get(&source.url).send().await.context("downloading ComfyUI")?;
    let resp = resp.error_for_status().context("downloading ComfyUI")?;
    let mut file = tokio::fs::File::create(dest).await.with_context(|| format!("creating {}", dest.display()))?;
    let mut hash = Sha256::new();
    let mut done = 0_u64;
    let mut reported = Instant::now();
    let mut stream = resp.bytes_stream();
    loop {
        let Some(chunk) = tokio::time::timeout(STALL, stream.next())
            .await
            .map_err(|_| anyhow!("the download stalled for a minute"))?
        else {
            break;
        };
        let chunk = chunk.context("downloading ComfyUI")?;
        hash.update(&chunk);
        file.write_all(&chunk).await.context("saving the download")?;
        done += chunk.len() as u64;
        if reported.elapsed() >= REPORT_EVERY {
            progress(done);
            reported = Instant::now();
        }
    }
    file.flush().await.context("saving the download")?;
    progress(done);
    let digest = hex(&hash.finalize());
    if done != source.size || !digest.eq_ignore_ascii_case(&source.sha256) {
        return Err(anyhow!(Mismatch)).with_context(|| format!("{done} bytes with SHA-256 {digest}"));
    }
    Ok(())
}

fn hex(digest: &[u8]) -> String {
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Whether `path` already holds the pinned archive, checked byte for byte. Blocking.
fn already_downloaded(path: &Path, size: u64, sha256: &str) -> bool {
    use std::io::Read;
    if std::fs::metadata(path).map(|m| m.len()).ok() != Some(size) {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hash.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    hex(&hash.finalize()).eq_ignore_ascii_case(sha256)
}

/// `std::fs::rename` that waits out a scanner holding files in `from` open.
fn rename_patiently(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut tries = 1;
    loop {
        match std::fs::rename(from, to) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && tries < MOVE_TRIES => {
                std::thread::sleep(MOVE_PAUSE);
                tries += 1;
            }
            done => return done,
        }
    }
}

/// Unpacks `archive` with 7za into `work`, moves the portable to `target`, and removes the other versions
/// beside it and the download folder. Blocking.
fn unpack_and_place(seven: &Path, archive: &Path, work: &Path, target: &Path) -> Result<()> {
    let unpacked = work.join("unpacked");
    // What an earlier, interrupted unpack left is not trusted.
    if unpacked.exists() {
        std::fs::remove_dir_all(&unpacked).with_context(|| format!("clearing {}", unpacked.display()))?;
    }
    let mut command = std::process::Command::new(seven);
    command
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", unpacked.display()))
        .arg(archive)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let out = command.output().context("running 7za")?;
    ensure!(out.status.success(), "unpacking ComfyUI failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    let portable = unpacked.join(PORTABLE_ROOT);
    ensure!(comfy::is_program(&portable), "the download doesn't hold ComfyUI's portable");
    let Some(versions) = target.parent() else { bail!("{} has no parent folder", target.display()) };
    std::fs::create_dir_all(versions).with_context(|| format!("creating {}", versions.display()))?;
    if target.exists() {
        std::fs::remove_dir_all(target).with_context(|| format!("clearing {}", target.display()))?;
    }
    rename_patiently(&portable, target).with_context(|| format!("moving ComfyUI to {}", target.display()))?;
    for entry in std::fs::read_dir(versions)?.filter_map(Result::ok) {
        let path = entry.path();
        if path != target
            && path.is_dir()
            && let Err(e) = std::fs::remove_dir_all(&path)
        {
            tracing::warn!(error = format!("{e:#}"), path = %path.display(), "removing an older ComfyUI");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener};

    /// A one-file web server on loopback: every request gets `status` and `body`.
    fn serve(status: &'static str, body: Vec<u8>) -> String {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port");
        let url = format!("http://127.0.0.1:{}/ComfyUI.7z", listener.local_addr().expect("an address").port());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().filter_map(Result::ok) {
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                let head = format!("HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        url
    }

    fn sha(bytes: &[u8]) -> String {
        crate::agent::registry::sha256_hex(bytes)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-comfy-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp folder");
        dir
    }

    #[tokio::test]
    async fn a_download_is_saved_checked_and_reported() {
        let body = b"pretend this is ComfyUI".repeat(1000);
        let source = Source { url: serve("200 OK", body.clone()), size: body.len() as u64, sha256: sha(&body) };
        let dir = temp("ok");
        let dest = dir.join("ComfyUI.7z");
        let last = std::sync::atomic::AtomicU64::new(0);
        download(&source, &dest, |done| last.store(done, std::sync::atomic::Ordering::SeqCst))
            .await
            .expect("downloaded");
        assert_eq!(std::fs::read(&dest).expect("saved"), body);
        assert_eq!(
            last.load(std::sync::atomic::Ordering::SeqCst),
            body.len() as u64,
            "the last report is the whole size"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_wrong_archive_is_refused_at_once_and_not_kept() {
        let body = b"not the pinned bytes".to_vec();
        let source = Source { url: serve("200 OK", body.clone()), size: body.len() as u64, sha256: sha(b"other") };
        let dir = temp("mismatch");
        let dest = dir.join("ComfyUI.7z");
        let started = Instant::now();
        let err = download(&source, &dest, |_| {}).await.expect_err("the hash differs");
        assert!(err.downcast_ref::<Mismatch>().is_some(), "{err:#}");
        assert!(started.elapsed() < Duration::from_secs(4), "a mismatch is not tried again");
        assert!(!dest.exists(), "a wrong archive is not left behind");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_missing_release_is_not_tried_again() {
        let source = Source { url: serve("404 Not Found", b"nope".to_vec()), size: 4, sha256: sha(b"nope") };
        let dir = temp("missing");
        let started = Instant::now();
        let err = download(&source, &dir.join("ComfyUI.7z"), |_| {}).await.expect_err("404");
        assert!(started.elapsed() < Duration::from_secs(4), "{err:#}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The desktop app's 7za (its `7zip-bin` devDependency), when its packages are installed.
    fn test_seven_zip() -> Option<PathBuf> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps/desktop/node_modules/7zip-bin/win/x64/7za.exe");
        path.is_file().then_some(path)
    }

    #[test]
    fn a_kept_archive_is_reused_only_when_it_is_the_pinned_one() {
        let dir = temp("kept");
        let path = dir.join("ComfyUI.7z");
        let body = b"the pinned archive".to_vec();
        std::fs::write(&path, &body).expect("an archive");
        assert!(already_downloaded(&path, body.len() as u64, &sha(&body)));
        assert!(!already_downloaded(&path, body.len() as u64, &sha(b"another")), "same size, other bytes");
        assert!(!already_downloaded(&path, 1, &sha(&body)), "other size");
        assert!(!already_downloaded(&dir.join("missing.7z"), 1, &sha(&body)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_archive_is_unpacked_into_its_version_folder_and_older_ones_go() {
        let Some(seven) = test_seven_zip() else {
            eprintln!("skipped: run `npm ci` in apps/desktop for 7za.exe");
            return;
        };
        let dir = temp("unpack");
        // A tiny archive with the official layout.
        let portable = dir.join("src").join(PORTABLE_ROOT);
        std::fs::create_dir_all(portable.join("ComfyUI")).expect("folders");
        std::fs::create_dir_all(portable.join("python_embeded")).expect("folders");
        std::fs::write(portable.join("ComfyUI").join("main.py"), "print('hi')").expect("main.py");
        std::fs::write(portable.join("python_embeded").join("python.exe"), "x").expect("python");
        let archive = dir.join("ComfyUI.7z");
        let made = std::process::Command::new(&seven)
            .arg("a")
            .arg(&archive)
            .arg(PORTABLE_ROOT)
            .current_dir(dir.join("src"))
            .output()
            .expect("7za runs");
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let versions = dir.join("program");
        let old = versions.join("v0.1.0");
        std::fs::create_dir_all(&old).expect("an older version");
        let target = versions.join("v0.44.0");

        unpack_and_place(&seven, &archive, &dir.join("work"), &target).expect("unpacked");

        assert!(comfy::is_program(&target));
        assert!(!old.exists(), "the older version is removed");
        let _ = std::fs::remove_dir_all(dir);
    }
}
