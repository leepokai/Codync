//! `codync-host`: runs coding-agent bots over ACP and serves the Codync apps.

mod acp;
mod api;
mod backends;
mod bot;
mod hub;
mod push;
mod registry;
mod service;
mod store;
mod usage;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fmt::Write as _;
use std::io::Read;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// Poison-tolerant locking. Every critical section in the host is a short,
/// self-contained update, so a panic elsewhere never leaves the data half
/// written — and a daemon must not wedge every later caller over one panic.
pub trait LockExt<T> {
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One shared HTTP client (connection pool + TLS config) for the whole process.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
    &CLIENT
}

#[derive(Parser)]
#[command(
    name = "codync-host",
    version,
    about = "Codync host: runs your coding-agent bots and serves the Codync phone app"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Sub>,
}

#[derive(Subcommand)]
enum Sub {
    /// Run the host in the foreground (what the background service runs).
    Serve {
        #[arg(long, default_value_t = service::DEFAULT_PORT)]
        port: u16,
        /// Address to bind. Defaults to all interfaces so the phone can reach it over Tailscale/LAN.
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
    },
    /// Show the pairing QR code for the Codync iOS app.
    Pair {
        #[arg(long, default_value_t = service::DEFAULT_PORT)]
        port: u16,
        /// Print machine-readable JSON instead of a QR code.
        #[arg(long)]
        json: bool,
    },
    /// Install and start the host as a background service (launchd / systemd --user).
    Install {
        #[arg(long, default_value_t = service::DEFAULT_PORT)]
        port: u16,
    },
    /// Stop and remove the background service.
    Uninstall,
    /// Show whether the host is installed and running.
    Status {
        #[arg(long, default_value_t = service::DEFAULT_PORT)]
        port: u16,
    },
    /// Claude Code statusLine command: forwards usage limits to the host, then prints
    /// the wrapped command's status line (`statusline -- <command>`) or a short default.
    Statusline {
        #[arg(long, default_value_t = service::DEFAULT_PORT)]
        port: u16,
        #[arg(last = true)]
        wrapped: Vec<String>,
    },
    /// Rotate the pairing token (unpairs every phone).
    ResetToken,
}

/// Host identity + auth token live in the database; the token is mirrored to
/// `~/.codync/token` (0600) for local helpers like the statusline command.
fn open_store() -> Result<(store::Store, String, String)> {
    let dir = service::data_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let db = dir.join("codync.db");
    let store = store::Store::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let host_id = if let Some(v) = store.kv_get("host_id") {
        v
    } else {
        let v = uuid::Uuid::new_v4().to_string();
        store.kv_set("host_id", &v)?;
        v
    };
    let token = match store.kv_get("token") {
        Some(v) => v,
        None => rotate_token(&store)?,
    };
    Ok((store, host_id, token))
}

fn rotate_token(store: &store::Store) -> Result<String> {
    let t = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    store.kv_set("token", &t)?;
    write_token_file(&t)?;
    Ok(t)
}

fn write_token_file(token: &str) -> Result<()> {
    let path = service::data_dir().join("token");
    std::fs::write(&path, token).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting {}", path.display()))?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Sub::Serve { port: service::DEFAULT_PORT, bind: "0.0.0.0".into() }) {
        Sub::Serve { port, bind } => serve(&bind, port).await,
        Sub::Pair { port, json } => {
            let (_, host_id, token) = open_store()?;
            let urls = service::addresses(port);
            let name = service::host_name();
            let url = service::pairing_url(&name, &token, &urls);
            if json {
                println!(
                    "{}",
                    json!({"name": name, "hostId": host_id, "token": token, "urls": urls, "pairingUrl": url})
                );
            } else {
                let code = qrcode::QrCode::new(url.as_bytes())?;
                println!("{}", code.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build());
                println!("Scan with the Codync iOS app, or open this link on the phone:\n{url}\n");
                if urls.is_empty() {
                    println!("⚠︎ No Tailscale or LAN address found — the phone won't be able to reach this machine.");
                } else if !urls.iter().any(|u| u.contains("://100.") || u.contains(".ts.net")) {
                    println!("Tip: install Tailscale on this machine and your phone to reach it from anywhere.");
                }
            }
            Ok(())
        }
        Sub::Install { port } => {
            open_store()?;
            if let Some(home) = dirs::home_dir() {
                match service::ensure_statusline(&home.join(".claude/settings.json")) {
                    Ok(true) => println!(
                        "Claude Code's status line now also reports usage limits to Codync (your own status line still shows)."
                    ),
                    Ok(false) => {}
                    Err(e) => eprintln!("Couldn't set Claude Code's status line: {e}"),
                }
            }
            service::install(port)?;
            println!("Codync host installed and started on port {port}. Run `codync-host pair` to connect your phone.");
            Ok(())
        }
        Sub::Uninstall => {
            service::uninstall();
            if let Some(home) = dirs::home_dir()
                && let Err(e) = service::restore_statusline(&home.join(".claude/settings.json"))
            {
                eprintln!("Couldn't restore Claude Code's status line: {e:#}");
            }
            println!("Codync host service removed. Data is kept in {}", service::data_dir().display());
            Ok(())
        }
        Sub::Status { port } => {
            let running = http()
                .get(format!("http://127.0.0.1:{port}/health"))
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success());
            println!(
                "installed: {}\nrunning:   {running}\nport:      {port}\ndata:      {}",
                service::installed(),
                service::data_dir().display()
            );
            Ok(())
        }
        Sub::Statusline { port, wrapped } => {
            statusline(port, wrapped.join(" ")).await;
            Ok(())
        }
        Sub::ResetToken => {
            let (store, _, _) = open_store()?;
            rotate_token(&store)?;
            println!("Token rotated. Restart the host and pair your phone again.");
            Ok(())
        }
    }
}

/// launchd appends to `host.log` forever. The service already holds it open as
/// stdout (append mode), so truncate in place rather than rotating.
fn cap_log() {
    let log = service::data_dir().join("host.log");
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > 10 * 1024 * 1024)
        && let Ok(f) = std::fs::OpenOptions::new().write(true).open(&log)
    {
        let _ = f.set_len(0);
    }
}

async fn serve(bind: &str, port: u16) -> Result<()> {
    cap_log();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "codync_host=info".into()),
        )
        .init();
    let (store, host_id, token) = open_store()?;
    write_token_file(&token)?;
    tokio::task::spawn_blocking(backends::hydrate_path).await?;
    let hub = hub::Hub::new(store, host_id, token, port);
    hub.start()?;
    tokio::spawn(registry::refresh_loop());
    tokio::spawn(usage::poll(hub.clone()));
    let listener =
        tokio::net::TcpListener::bind((bind, port)).await.with_context(|| format!("binding {bind}:{port}"))?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), bind, port, "codync-host listening");
    axum::serve(listener, api::router(hub.clone())).with_graceful_shutdown(shutdown_signal()).await?;
    // launchd/systemd stop us with SIGTERM: stop every agent instead of orphaning it.
    hub.shutdown().await;
    tracing::info!("codync-host stopped");
    Ok(())
}

/// Resolves on Ctrl-C or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "can't listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "can't listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
}

/// Never blocks Claude Code: 1 s budget, all errors ignored.
async fn statusline(port: u16, wrapped: String) {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    if let Ok(token) = std::fs::read_to_string(service::data_dir().join("token")) {
        let _ = http()
            .post(format!("http://127.0.0.1:{port}/ingest/statusline"))
            .bearer_auth(token.trim())
            .json(&v)
            .timeout(Duration::from_secs(1))
            .send()
            .await;
    }
    if !wrapped.trim().is_empty() {
        // The user's own status line gets the same JSON on stdin.
        use std::io::Write;
        if let Ok(mut child) =
            std::process::Command::new("/bin/sh").arg("-c").arg(&wrapped).stdin(std::process::Stdio::piped()).spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(input.as_bytes());
            }
            let _ = child.wait();
        }
        return;
    }
    let model = v["model"]["display_name"].as_str().unwrap_or("Claude");
    let mut line = model.to_owned();
    for (k, label) in [("five_hour", "5h"), ("seven_day", "7d")] {
        if let Some(p) = v["rate_limits"][k]["used_percentage"].as_f64() {
            let _ = write!(line, " · {label} {p:.0}%");
        }
    }
    println!("{line}");
}
