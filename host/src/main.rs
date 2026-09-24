mod acp;
mod api;
mod backends;
mod bot;
mod hub;
mod push;
mod service;
mod store;
mod usage;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use std::io::Read;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "codync-host", version, about = "Codync host: runs your coding-agent bots and serves the Codync phone app")]
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
    /// Claude Code statusLine command: forwards usage limits to the host and prints a short status line.
    Statusline {
        #[arg(long, default_value_t = service::DEFAULT_PORT)]
        port: u16,
    },
    /// Rotate the pairing token (unpairs every phone).
    ResetToken,
}

/// Host identity + auth token live in the database; the token is mirrored to
/// `~/.codync/token` (0600) for local helpers like the statusline command.
fn open_store() -> Result<(store::Store, String, String)> {
    let dir = service::data_dir();
    std::fs::create_dir_all(&dir)?;
    let store = store::Store::open(&dir.join("codync.db"))?;
    let host_id = match store.kv_get("host_id") {
        Some(v) => v,
        None => {
            let v = uuid::Uuid::new_v4().to_string();
            store.kv_set("host_id", &v)?;
            v
        }
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
    std::fs::write(&path, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
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
                println!("{}", json!({"name": name, "hostId": host_id, "token": token, "urls": urls, "pairingUrl": url}));
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
                match service::remove_legacy_hooks(&home.join(".claude/settings.json")) {
                    Ok(0) => {}
                    Ok(n) => println!("Removed {n} Codync 1.x hook(s) from ~/.claude/settings.json (backup saved next to it)."),
                    Err(e) => eprintln!("Couldn't clean up Codync 1.x hooks: {e}"),
                }
            }
            service::install(port)?;
            println!("Codync host installed and started on port {port}. Run `codync-host pair` to connect your phone.");
            Ok(())
        }
        Sub::Uninstall => {
            service::uninstall()?;
            println!("Codync host service removed. Data is kept in {}", service::data_dir().display());
            Ok(())
        }
        Sub::Status { port } => {
            let running = reqwest::Client::new()
                .get(format!("http://127.0.0.1:{port}/health"))
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success());
            println!("installed: {}\nrunning:   {running}\nport:      {port}\ndata:      {}", service::installed(), service::data_dir().display());
            Ok(())
        }
        Sub::Statusline { port } => {
            statusline(port).await;
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

async fn serve(bind: &str, port: u16) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "codync_host=info".into()),
        )
        .init();
    let (store, host_id, token) = open_store()?;
    write_token_file(&token)?;
    let hub = hub::Hub::new(store, host_id, token);
    hub.start()?;
    tokio::spawn(usage::poll(hub.clone()));
    let listener = tokio::net::TcpListener::bind((bind, port)).await?;
    tracing::info!("codync-host {} listening on {bind}:{port}", env!("CARGO_PKG_VERSION"));
    axum::serve(listener, api::router(hub)).await?;
    Ok(())
}

/// Never blocks Claude Code: 1 s budget, all errors ignored.
async fn statusline(port: u16) {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    if let Ok(token) = std::fs::read_to_string(service::data_dir().join("token")) {
        let _ = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/ingest/statusline"))
            .bearer_auth(token.trim())
            .json(&v)
            .timeout(Duration::from_secs(1))
            .send()
            .await;
    }
    let model = v["model"]["display_name"].as_str().unwrap_or("Claude");
    let mut line = model.to_owned();
    for (k, label) in [("five_hour", "5h"), ("seven_day", "7d")] {
        if let Some(p) = v["rate_limits"][k]["used_percentage"].as_f64() {
            line.push_str(&format!(" · {label} {p:.0}%"));
        }
    }
    println!("{line}");
}
