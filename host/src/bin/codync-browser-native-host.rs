//! Chrome native-messaging relay for the Codync Browser extension.
//!
//! Chrome owns stdin/stdout in the native-messaging protocol. This process reads the same-user
//! loopback token, opens the authenticated WebSocket, and relays only protocol frames. The token
//! never crosses into extension JavaScript.

use anyhow::{Context as _, Result, bail, ensure};
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::io::{Read, Write as _};
#[cfg(any(windows, test))]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Message, http::header};

const MAX_FRAME: usize = 32 * 1024 * 1024;
const INSTALL_COMMAND: &str = "--install-native-messaging";
const UNINSTALL_COMMAND: &str = "--uninstall-native-messaging";
const BROWSER_PROFILE_OWNER_ENV: &str = "CODYNC_BROWSER_PROFILE_OWNER";
#[cfg(any(windows, test))]
const EXTENSION_ID: &str = "dgejialhpalhoigbkmgeabgpphlipiek";
#[cfg(windows)]
const CHROME_REGISTRY: &str = r"HKCU\Software\Google\Chrome\NativeMessagingHosts";
#[cfg(windows)]
const EDGE_REGISTRY: &str = r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts";

#[tokio::main]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        Some(INSTALL_COMMAND) => {
            if let Err(error) = install_native_messaging() {
                eprintln!("codync-browser-native-host: {error:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        Some(UNINSTALL_COMMAND) => {
            if let Err(error) = uninstall_native_messaging() {
                eprintln!("codync-browser-native-host: {error:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        _ => {}
    }
    if let Err(error) = run().await {
        let _ = write_native(&json!({
            "type": "bridgeError",
            "error": format!("{error:#}")
        }));
        eprintln!("codync-browser-native-host: {error:#}");
        std::process::exit(1);
    }
    // Chrome owns stdin. If Chrome remains open after the host WebSocket closes, aborting a
    // spawn_blocking stdin read does not interrupt the blocked read and Tokio waits for it while
    // dropping the runtime. Exit the helper so Chrome sees the native port close and reconnects
    // when Codync is available again.
    std::process::exit(0);
}

async fn run() -> Result<()> {
    let profile_owner = std::env::var(BROWSER_PROFILE_OWNER_ENV).ok();
    let profile = profile_ownership(profile_owner.as_deref(), option_env!("CODYNC_ENV"));
    let first = tokio::task::spawn_blocking(|| {
        let stdin = std::io::stdin();
        read_native(&mut stdin.lock())
    })
    .await
    .context("reading Chrome's initial browser bridge message")??;
    let Some(first) = first else {
        return Ok(());
    };
    match profile {
        ProfileOwnership::Other => {
            write_native(&json!({
                "type": "bridgeError",
                "error": "This Chrome window belongs to the other Codync variant."
            }))?;
            return Ok(());
        }
        ProfileOwnership::Normal => {
            launch_isolated_browser().context("opening Codync Browser's dedicated profile")?;
            write_native(&json!({
                "type": "bridgeError",
                "error": "Opened Codync Browser in its dedicated profile. Use that window for bot browser tabs."
            }))?;
            return Ok(());
        }
        ProfileOwnership::Owned => {}
    }
    let first = relay_message(&first, true)?;

    let token = std::fs::read_to_string(token_path()?).context("reading the Codync loopback token")?.trim().to_owned();
    if token.is_empty() {
        bail!("The Codync loopback token is empty");
    }
    let url = host_url()?;
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
    let (socket, _) = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(request))
        .await
        .context("Connecting to Codync timed out")?
        .context("Connecting to Codync")?;
    let (mut sink, mut stream) = socket.split();
    sink.send(Message::Text(first.into())).await?;

    let (from_chrome, mut chrome) = mpsc::channel::<String>(32);
    let reader = tokio::task::spawn_blocking(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            let Some(message) = read_native(&mut input)? else {
                return Ok::<_, anyhow::Error>(());
            };
            if from_chrome.blocking_send(message).is_err() {
                return Ok(());
            }
        }
    });

    loop {
        tokio::select! {
            message = chrome.recv() => {
                let Some(message) = message else { break };
                sink.send(Message::Text(message.into())).await?;
            }
            message = stream.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => write_native_text(text.as_str())?,
                    Some(Ok(Message::Binary(bytes))) => {
                        write_native_text(std::str::from_utf8(&bytes).context("Codync sent non-UTF-8 browser JSON")?)?;
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(error)) => return Err(error).context("Codync browser WebSocket"),
                }
            }
        }
    }
    drop(chrome);
    reader.abort();
    Ok(())
}

fn relay_message(message: &str, isolated_profile: bool) -> Result<String> {
    let mut value: Value = serde_json::from_str(message).context("Chrome sent invalid browser bridge JSON")?;
    if value["type"] == "hello" {
        // The helper, not extension JavaScript, owns this bit. A normal browser profile cannot
        // claim that it is the dedicated Codync profile.
        value["isolatedProfile"] = Value::Bool(isolated_profile);
    }
    Ok(value.to_string())
}

#[cfg(any(windows, test))]
fn native_host() -> &'static str {
    native_host_for(option_env!("CODYNC_ENV"))
}

#[cfg(any(windows, test))]
fn native_host_for(build_env: Option<&str>) -> &'static str {
    match build_env {
        Some("main") => "codync.browser",
        _ => "codync.browser.dev",
    }
}

#[cfg(windows)]
fn native_manifest_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("finding the local application data directory")?
        .join(app_folder(option_env!("CODYNC_ENV")))
        .join("BrowserExtension"))
}

#[cfg(any(windows, test))]
fn app_folder(build_env: Option<&str>) -> &'static str {
    match build_env {
        Some("main") => "Codync",
        _ => "Codync Dev",
    }
}

fn profile_owner(build_env: Option<&str>) -> &'static str {
    match build_env {
        Some("main") => "main",
        _ => "dev",
    }
}

fn profile_owner_matches(owner: Option<&str>, build_env: Option<&str>) -> bool {
    owner == Some(profile_owner(build_env))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileOwnership {
    Normal,
    Owned,
    Other,
}

fn profile_ownership(owner: Option<&str>, build_env: Option<&str>) -> ProfileOwnership {
    match owner {
        None => ProfileOwnership::Normal,
        Some(_) if profile_owner_matches(owner, build_env) => ProfileOwnership::Owned,
        Some(_) => ProfileOwnership::Other,
    }
}

#[cfg(windows)]
fn launch_isolated_browser() -> Result<()> {
    use std::process::{Command, Stdio};

    let extension = extension_dir()?;
    ensure!(
        extension.join("manifest.json").is_file(),
        "the packaged Codync Browser extension is missing from {}",
        extension.display()
    );
    let profile = native_manifest_dir()?.join("DedicatedProfile");
    std::fs::create_dir_all(&profile)
        .with_context(|| format!("creating the dedicated browser profile at {}", profile.display()))?;
    let marker = profile.join(".last-launch");
    let recently_opened = std::fs::metadata(&marker)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed < Duration::from_secs(15));
    if recently_opened {
        return Ok(());
    }
    std::fs::write(&marker, b"launched")
        .with_context(|| format!("recording the dedicated browser launch at {}", marker.display()))?;

    let executable = browser_executable()?;
    let extension_arg = format!("--load-extension={}", extension.display());
    let only_extension_arg = format!("--disable-extensions-except={}", extension.display());
    let profile_arg = format!("--user-data-dir={}", profile.display());
    Command::new(executable)
        .args([
            profile_arg.as_str(),
            extension_arg.as_str(),
            only_extension_arg.as_str(),
            "--no-first-run",
            "--no-default-browser-check",
            "--new-window",
            "about:blank",
        ])
        .env(BROWSER_PROFILE_OWNER_ENV, profile_owner(option_env!("CODYNC_ENV")))
        .env("CODYNC_BROWSER_EXTENSION_DIR", &extension)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("starting Chrome for Codync Browser")?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_isolated_browser() -> Result<()> {
    bail!("the Codync Browser native host is available on Windows only")
}

#[cfg(windows)]
fn extension_dir() -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("CODYNC_BROWSER_EXTENSION_DIR") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        candidates.extend(extension_candidates(parent));
    }
    for candidate in candidates {
        if let Ok(canonical) = candidate.canonicalize()
            && canonical.join("manifest.json").is_file()
        {
            return Ok(canonical);
        }
    }
    bail!("the packaged Codync Browser extension directory was not found")
}

#[cfg(any(windows, test))]
fn extension_candidates(parent: &Path) -> Vec<PathBuf> {
    vec![
        parent.join("browser-extension"),
        parent.join("../../../apps/browser-extension"),
        parent.join("../../../browser-extension"),
    ]
}

#[cfg(windows)]
fn browser_executable() -> Result<PathBuf> {
    let override_bin = std::env::var_os("CODYNC_BROWSER_BIN").map(PathBuf::from);
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(dirs::data_local_dir);
    let program_files =
        Some(std::env::var_os("ProgramFiles").map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from));
    let program_files_x86 = Some(
        std::env::var_os("ProgramFiles(x86)").map_or_else(|| PathBuf::from(r"C:\Program Files (x86)"), PathBuf::from),
    );
    let registry_paths = registered_browser_paths();
    resolve_browser_executable(
        override_bin.as_deref(),
        local_app_data.as_deref(),
        program_files.as_deref(),
        program_files_x86.as_deref(),
        &registry_paths,
        Path::is_file,
    )
}

#[cfg(windows)]
fn registered_browser_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for exe in ["chrome.exe", "msedge.exe", "brave.exe"] {
        let subkey = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
        for root in [windows_registry::CURRENT_USER, windows_registry::LOCAL_MACHINE] {
            if let Ok(key) = root.open(&subkey)
                && let Ok(raw) = key.get_string("")
                && let Some(parsed) = parse_registry_executable(&raw)
            {
                paths.push(parsed);
            }
        }
    }
    paths
}

#[cfg(any(windows, test))]
fn parse_registry_executable(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim().trim_matches('"');
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

#[cfg(any(windows, test))]
fn browser_executable_candidates(
    local_app_data: Option<&Path>,
    program_files: Option<&Path>,
    program_files_x86: Option<&Path>,
    registry_paths: &[PathBuf],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for root in [program_files, program_files_x86].into_iter().flatten() {
        candidates.push(root.join(r"Google\Chrome\Application\chrome.exe"));
    }
    for root in [program_files, program_files_x86].into_iter().flatten() {
        candidates.push(root.join(r"Microsoft\Edge\Application\msedge.exe"));
    }
    if let Some(local) = local_app_data {
        candidates.push(local.join(r"Google\Chrome\Application\chrome.exe"));
        candidates.push(local.join(r"Microsoft\Edge\Application\msedge.exe"));
        candidates.push(local.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"));
    }
    for root in [program_files, program_files_x86].into_iter().flatten() {
        candidates.push(root.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"));
    }
    candidates.extend(registry_paths.iter().cloned());
    candidates
}

#[cfg(any(windows, test))]
fn resolve_browser_executable(
    override_bin: Option<&Path>,
    local_app_data: Option<&Path>,
    program_files: Option<&Path>,
    program_files_x86: Option<&Path>,
    registry_paths: &[PathBuf],
    is_file: impl Fn(&Path) -> bool,
) -> Result<PathBuf> {
    if let Some(path) = override_bin {
        ensure!(is_file(path), "CODYNC_BROWSER_BIN does not name a file: {}", path.display());
        return Ok(path.to_owned());
    }
    for candidate in browser_executable_candidates(local_app_data, program_files, program_files_x86, registry_paths) {
        if is_file(&candidate) {
            return Ok(candidate);
        }
    }
    bail!("Chrome or Edge was not found for Codync Browser")
}

fn host_url() -> Result<String> {
    if let Some(url) = std::env::var_os("CODYNC_BROWSER_HOST_URL") {
        let url = url.into_string().map_err(|_| anyhow::anyhow!("CODYNC_BROWSER_HOST_URL is not UTF-8"))?;
        return checked_host_url(&url);
    }
    let port = match option_env!("CODYNC_ENV") {
        Some("main") => 19222,
        _ => 19223,
    };
    Ok(format!("ws://127.0.0.1:{port}/browser-extension"))
}

fn checked_host_url(raw: &str) -> Result<String> {
    let url = url::Url::parse(raw).context("CODYNC_BROWSER_HOST_URL is invalid")?;
    ensure!(url.scheme() == "ws", "CODYNC_BROWSER_HOST_URL must use ws");
    ensure!(
        url.host_str() == Some("127.0.0.1"),
        "CODYNC_BROWSER_HOST_URL must use the IPv4 loopback address 127.0.0.1"
    );
    ensure!(url.path() == "/browser-extension", "CODYNC_BROWSER_HOST_URL must use /browser-extension");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "CODYNC_BROWSER_HOST_URL cannot contain credentials"
    );
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "CODYNC_BROWSER_HOST_URL cannot contain a query or fragment"
    );
    Ok(url.to_string())
}

fn token_path() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("CODYNC_HOME") {
        return Ok(PathBuf::from(home).join("token"));
    }
    let folder = match option_env!("CODYNC_ENV") {
        Some("main") => ".codync",
        _ => ".codync-dev",
    };
    let home = dirs::home_dir().context("finding the user's home directory")?;
    Ok(home.join(folder).join("token"))
}

#[cfg(windows)]
fn install_native_messaging() -> Result<()> {
    use std::process::{Command, Stdio};

    let executable = std::env::current_exe()
        .context("finding the Codync Browser native host")?
        .canonicalize()
        .context("resolving the Codync Browser native host")?;
    let manifest_dir = native_manifest_dir()?;
    std::fs::create_dir_all(&manifest_dir)?;
    let manifest_path = manifest_dir.join(format!("{}.json", native_host()));
    let manifest = native_manifest(&executable, EXTENSION_ID);
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    for root in [CHROME_REGISTRY, EDGE_REGISTRY] {
        let key = format!(r"{root}\{}", native_host());
        let output = Command::new("reg.exe")
            .args(["add", &key, "/ve", "/t", "REG_SZ", "/d", manifest_path.to_string_lossy().as_ref(), "/f"])
            .stdin(Stdio::null())
            .output()
            .context("starting reg.exe to register the Codync Browser bridge")?;
        ensure!(
            output.status.success(),
            "reg.exe could not register {key}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_native_messaging() -> Result<()> {
    bail!("the Codync Browser native host is available on Windows only")
}

#[cfg(windows)]
fn uninstall_native_messaging() -> Result<()> {
    let host_name = native_host();
    for root in [CHROME_REGISTRY, EDGE_REGISTRY] {
        delete_native_messaging_key(root, host_name)?;
    }
    let manifest_dir = native_manifest_dir()?;
    let manifest_path = manifest_dir.join(format!("{}.json", native_host()));
    match std::fs::remove_file(&manifest_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("removing {}", manifest_path.display())),
    }
    match std::fs::remove_dir(&manifest_dir) {
        Ok(()) => {}
        Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty) => {}
        Err(error) => return Err(error).with_context(|| format!("removing {}", manifest_dir.display())),
    }
    Ok(())
}

#[cfg(windows)]
fn delete_native_messaging_key(root: &str, host_name: &str) -> Result<()> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyW};

    let key = format!(r"{root}\{host_name}");
    let subkey = key.strip_prefix(r"HKCU\").with_context(|| format!("unexpected current-user registry path {key}"))?;
    let wide = subkey.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
    // SAFETY: `wide` is a live, NUL-terminated UTF-16 path and `HKEY_CURRENT_USER` is a valid root key.
    let status = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide.as_ptr()) };
    if registry_delete_completed(status) {
        return Ok(());
    }
    Err(std::io::Error::from_raw_os_error(status.cast_signed())).with_context(|| format!("removing registry key {key}"))
}

#[cfg(windows)]
fn registry_delete_completed(status: u32) -> bool {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS};

    status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND
}

#[cfg(not(windows))]
fn uninstall_native_messaging() -> Result<()> {
    bail!("the Codync Browser native host is available on Windows only")
}

#[cfg(any(windows, test))]
fn native_manifest(executable: &Path, extension_id: &str) -> Value {
    json!({
        "name": native_host(),
        "description": "Codync Browser native-messaging bridge",
        "path": native_manifest_path(executable),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension_id}/")]
    })
}

#[cfg(any(windows, test))]
fn native_manifest_path(executable: &Path) -> String {
    // Chrome's Windows native-messaging launcher rejects the `\\?\` NT namespace prefix that
    // `canonicalize` adds, even though the same path opens normally in Rust and Explorer.
    let path = executable.to_string_lossy();
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else if let Some(path) = path.strip_prefix(r"\\?\") {
        path.to_owned()
    } else {
        path.into_owned()
    }
}

fn read_native(input: &mut impl Read) -> Result<Option<String>> {
    let mut header = [0_u8; 4];
    match input.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("reading Chrome's native-message header"),
    }
    let length = u32::from_le_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME {
        bail!("Chrome sent a browser bridge frame larger than {MAX_FRAME} bytes");
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body).context("reading Chrome's native-message body")?;
    Ok(Some(String::from_utf8(body).context("Chrome sent non-UTF-8 browser JSON")?))
}

fn write_native_text(text: &str) -> Result<()> {
    write_native_raw(text.as_bytes())
}

fn write_native(value: &Value) -> Result<()> {
    write_native_raw(value.to_string().as_bytes())
}

fn write_native_raw(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        bail!("native browser message is empty or too large");
    }
    let length = u32::try_from(bytes.len()).context("native browser message length")?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    output.write_all(&length.to_le_bytes())?;
    output.write_all(bytes)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::ffi::OsStr;
    use std::io::Cursor;

    #[test]
    fn reads_a_chrome_length_prefixed_message() {
        let body = br#"{"type":"hello"}"#;
        let mut framed = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
        framed.extend_from_slice(body);
        assert_eq!(read_native(&mut Cursor::new(framed)).unwrap().as_deref(), Some(r#"{"type":"hello"}"#));
    }

    #[test]
    fn eof_is_a_clean_shutdown() {
        assert!(read_native(&mut Cursor::new(Vec::<u8>::new())).unwrap().is_none());
    }

    #[test]
    fn rejects_an_oversized_frame() {
        let mut framed = u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes().to_vec();
        framed.extend_from_slice(OsStr::new("ignored").as_encoded_bytes());
        assert!(read_native(&mut Cursor::new(framed)).is_err());
    }

    #[test]
    fn native_manifest_serializes_windows_paths_as_json() {
        let manifest =
            native_manifest(Path::new(r"C:\Users\Example User\Codync\codync-browser-native-host.exe"), EXTENSION_ID);
        let encoded = serde_json::to_vec(&manifest).unwrap();
        let decoded: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded["name"], native_host());
        assert_eq!(decoded["path"], r"C:\Users\Example User\Codync\codync-browser-native-host.exe");
        assert_eq!(decoded["allowed_origins"][0], format!("chrome-extension://{EXTENSION_ID}/"));
    }

    #[test]
    fn native_manifest_removes_windows_verbatim_path_prefixes() {
        let drive_path =
            native_manifest_path(Path::new(r"\\?\C:\Users\Example User\Codync\codync-browser-native-host.exe"));
        assert_eq!(drive_path, r"C:\Users\Example User\Codync\codync-browser-native-host.exe");

        let unc_path = native_manifest_path(Path::new(r"\\?\UNC\server\share\Codync\codync-browser-native-host.exe"));
        assert_eq!(unc_path, r"\\server\share\Codync\codync-browser-native-host.exe");
    }

    #[test]
    fn browser_host_override_must_be_authenticated_loopback() {
        assert_eq!(
            checked_host_url("ws://127.0.0.1:19333/browser-extension").unwrap(),
            "ws://127.0.0.1:19333/browser-extension"
        );
        for invalid in [
            "wss://127.0.0.1:19333/browser-extension",
            "ws://localhost:19333/browser-extension",
            "ws://192.0.2.1:19333/browser-extension",
            "ws://127.0.0.1:19333/other",
            "ws://token@127.0.0.1:19333/browser-extension",
        ] {
            assert!(checked_host_url(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn bridge_relays_the_helper_owned_profile_marker() {
        let hello = r#"{"type":"hello","isolatedProfile":true}"#;
        let isolated: Value = serde_json::from_str(&relay_message(hello, true).unwrap()).unwrap();
        assert_eq!(isolated["isolatedProfile"], true);

        let normal: Value = serde_json::from_str(&relay_message(hello, false).unwrap()).unwrap();
        assert_eq!(normal["isolatedProfile"], false);
    }

    #[test]
    fn development_and_release_hosts_have_separate_installations() {
        assert_eq!(native_host_for(Some("main")), "codync.browser");
        assert_eq!(native_host_for(Some("dev")), "codync.browser.dev");
        assert_eq!(app_folder(Some("main")), "Codync");
        assert_eq!(app_folder(Some("dev")), "Codync Dev");
    }

    #[test]
    fn profile_owner_marker_is_bound_to_the_helper_variant() {
        assert!(profile_owner_matches(Some("main"), Some("main")));
        assert!(profile_owner_matches(Some("dev"), Some("dev")));
        assert!(profile_owner_matches(Some("dev"), None));
        assert!(!profile_owner_matches(Some("dev"), Some("main")));
        assert!(!profile_owner_matches(Some("main"), Some("dev")));
        assert!(!profile_owner_matches(None, Some("main")));
    }

    #[test]
    fn profile_ownership_rejects_the_other_variant_without_relaunching() {
        assert_eq!(profile_ownership(None, Some("main")), ProfileOwnership::Normal);
        assert_eq!(profile_ownership(Some("main"), Some("main")), ProfileOwnership::Owned);
        assert_eq!(profile_ownership(Some("dev"), None), ProfileOwnership::Owned);
        assert_eq!(profile_ownership(Some("dev"), Some("main")), ProfileOwnership::Other);
        assert_eq!(profile_ownership(Some("main"), Some("dev")), ProfileOwnership::Other);
        assert_eq!(profile_ownership(Some("unknown"), Some("dev")), ProfileOwnership::Other);
    }

    #[test]
    #[cfg(windows)]
    fn registry_delete_only_ignores_a_missing_key() {
        use windows_sys::Win32::Foundation::{
            ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
        };

        assert!(registry_delete_completed(ERROR_SUCCESS));
        assert!(registry_delete_completed(ERROR_FILE_NOT_FOUND));
        assert!(registry_delete_completed(ERROR_PATH_NOT_FOUND));
        assert!(!registry_delete_completed(ERROR_ACCESS_DENIED));
    }

    #[test]
    fn local_installation_finds_the_workspace_extension() {
        let release = Path::new(r"C:\Codync\host\target\release");
        assert!(
            extension_candidates(release).contains(&release.join("../../../apps/browser-extension")),
            "the documented development layout must be searched"
        );
    }

    #[test]
    fn discovers_per_user_chrome_in_local_app_data() {
        let local = Path::new(r"C:\Users\Alice\AppData\Local");
        let per_user = local.join(r"Google\Chrome\Application\chrome.exe");
        let found = resolve_browser_executable(
            None,
            Some(local),
            Some(Path::new(r"C:\Program Files")),
            Some(Path::new(r"C:\Program Files (x86)")),
            &[],
            |path| path == per_user,
        )
        .unwrap();
        assert_eq!(found, per_user);
    }

    #[test]
    fn discovers_browser_from_app_paths_registry_and_strips_quotes() {
        let registered = parse_registry_executable(r#"  "D:\Custom\Chrome\chrome.exe"  "#).unwrap();
        assert_eq!(registered, PathBuf::from(r"D:\Custom\Chrome\chrome.exe"));
        assert!(parse_registry_executable(r#"   ""   "#).is_none());

        let found = resolve_browser_executable(
            None,
            Some(Path::new(r"C:\Users\Alice\AppData\Local")),
            Some(Path::new(r"C:\Program Files")),
            None,
            std::slice::from_ref(&registered),
            |path| path == registered,
        )
        .unwrap();
        assert_eq!(found, registered);
    }

    #[test]
    fn explicit_override_wins_and_missing_override_fails() {
        let override_bin = Path::new(r"D:\Portable\chrome.exe");
        let per_user = Path::new(r"C:\Users\Alice\AppData\Local").join(r"Google\Chrome\Application\chrome.exe");
        let found = resolve_browser_executable(
            Some(override_bin),
            Some(Path::new(r"C:\Users\Alice\AppData\Local")),
            None,
            None,
            &[],
            |path| path == override_bin || path == per_user,
        )
        .unwrap();
        assert_eq!(found, override_bin);

        let err = resolve_browser_executable(
            Some(Path::new(r"D:\Missing\chrome.exe")),
            Some(Path::new(r"C:\Users\Alice\AppData\Local")),
            None,
            None,
            &[],
            |path| path == per_user,
        )
        .unwrap_err();
        assert!(err.to_string().contains("CODYNC_BROWSER_BIN"));
    }

    #[test]
    fn missing_browser_returns_explicit_error() {
        let err = resolve_browser_executable(None, None, None, None, &[], |_| false).unwrap_err();
        assert!(err.to_string().contains("Chrome or Edge was not found"));
    }
}
