//! `codync-host pair` and `codync-host access`: letting phones and devices in from a terminal. A computer in no
//! account asks before a signed-in phone comes in and brings it into its account (docs/plans/require-sign-in.md,
//! phase 2): `pair` answers in a terminal, `access approve` / `deny` anywhere (scripts, SSH without a terminal).

use crate::local_api::local_call;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::io::{IsTerminal as _, Write as _};
use std::time::Duration;

#[derive(clap::Subcommand)]
pub enum AccessAction {
    /// Show pending requests (account devices asking for access, phones asking to pair) with the code each device
    /// shows (the default).
    List,
    /// Approve a request, by its id or by the 6-digit code shown on the device.
    Approve { request: String },
    /// Deny a request.
    Deny { request: String },
}

const ASKS: &str = "This computer isn't in an account, so it asks before a phone comes in. Answer within 90 seconds \
                    with `codync-host access approve <code>` (the 6 digits the phone shows), or in the Codync desktop \
                    app or the terminal UI.";

pub async fn pair(port: u16, as_json: bool) -> Result<()> {
    let hello = local_call(port, "hello", json!({})).await?;
    let p = local_call(port, "pairing", json!({"shortCode": true})).await?;
    let asks = local_call(port, "cloudStatus", json!({})).await?["owner"].is_null();
    if as_json {
        println!(
            "{}",
            json!({
                "name": hello["name"], "computerId": hello["computerId"], "signKey": hello["signKey"],
                "boxKey": hello["boxKey"], "urls": p["urls"], "cloud": hello["cloud"],
                "pairingUrl": p["pairingUrl"], "expiresAt": p["expiresAt"], "shortCode": p["shortCode"],
            })
        );
        if asks {
            eprintln!("{ASKS}");
        }
        return Ok(());
    }
    let url = p["pairingUrl"].as_str().unwrap_or_default();
    let code = qrcode::QrCode::new(url.as_bytes())?;
    println!("{}", code.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build());
    println!("Scan with the Codync iOS app within 10 minutes, or open this link on the phone:\n{url}\n");
    if let Some(code) = p["shortCode"].as_str() {
        println!("Or type this code where the app's pairing screen asks for a link: {code}\n");
    }
    if hello["cloud"].is_null()
        && p["urls"].as_array().is_none_or(|u| {
            !u.iter().any(|u| {
                u.as_str().is_some_and(|u| u.contains("://100.") || u.contains(".ts.net") || u.starts_with("https://"))
            })
        })
    {
        println!(
            "Tip: the phone reaches this computer on the same network only. Turn on the Codync cloud or Tailscale to reach it from anywhere."
        );
    }
    if !std::io::stdin().is_terminal() {
        if asks {
            println!("{ASKS}");
        }
        return Ok(());
    }
    wait(port, url, p["expiresAt"].as_i64().unwrap_or_default()).await
}

/// In a terminal, `pair` stays until its code is used or expires, and answers the phone the computer asks about.
async fn wait(port: u16, url: &str, expires_at: i64) -> Result<()> {
    let code = reqwest::Url::parse(url)?
        .query_pairs()
        .find_map(|(k, v)| (k == "code").then(|| v.into_owned()))
        .unwrap_or_default();
    println!("Waiting for the phone… (Ctrl-C to stop)");
    while crate::store::now_ms() < expires_at {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let requests = local_call(port, "pairingRequests", json!({})).await?;
        if let Some(r) = requests["requests"].as_array().and_then(|r| r.first()) {
            println!(
                "\n{} wants to use this computer. It joins {}'s account.\nCheck the phone shows {}.",
                r["deviceName"].as_str().unwrap_or("A phone"),
                r["email"].as_str().unwrap_or("a Codync account"),
                spaced(r["code"].as_str().unwrap_or_default()),
            );
            let approve = ask_yes_no("Let it in? [y/N] ").await?;
            let answered = local_call(port, "decidePairing", json!({"requestId": r["requestId"], "approve": approve}));
            if answered.await.is_err() {
                println!("The phone stopped waiting, or it was answered elsewhere.");
            } else if !approve {
                println!("Turned away.");
                return Ok(());
            }
        }
        if local_call(port, "pairingCodeOpen", json!({"code": code})).await?["open"] != true {
            println!("Paired.");
            return Ok(());
        }
    }
    println!("The code expired. Run codync-host pair again.");
    Ok(())
}

/// `123 456`, as the phone shows it.
fn spaced(code: &str) -> String {
    match code.split_at_checked(3) {
        Some((a, b)) if code.len() == 6 => format!("{a} {b}"),
        _ => code.to_owned(),
    }
}

async fn ask_yes_no(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let line = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).map(|_| line)
    })
    .await??;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

/// Requests waiting for this computer's answer, each with the method that answers it.
async fn requests(port: u16) -> Result<Vec<(Value, &'static str)>> {
    let mut all = vec![];
    for (list, decide) in [("accessRequests", "decideAccessRequest"), ("pairingRequests", "decidePairing")] {
        let v = local_call(port, list, json!({})).await?;
        all.extend(v["requests"].as_array().into_iter().flatten().map(|r| (r.clone(), decide)));
    }
    Ok(all)
}

pub async fn access(port: u16, action: AccessAction) -> Result<()> {
    let requests = requests(port).await?;
    let (request, approve) = match action {
        AccessAction::List => {
            if requests.is_empty() {
                println!("No devices are asking for access.");
            }
            for (r, decide) in &requests {
                println!(
                    "{}  {} ({}, {})  code {}{}",
                    r["requestId"].as_str().unwrap_or_default(),
                    r["deviceName"].as_str().unwrap_or_default(),
                    r["platform"].as_str().unwrap_or_default(),
                    r["email"].as_str().unwrap_or("no email"),
                    r["code"].as_str().unwrap_or("(waiting for the device)"),
                    if *decide == "decidePairing" { "  pairing: the computer joins this account" } else { "" },
                );
            }
            return Ok(());
        }
        AccessAction::Approve { request } => (request, true),
        AccessAction::Deny { request } => (request, false),
    };
    // A 6-digit code picks the request showing it; it must be the only one.
    let matching: Vec<_> =
        requests.iter().filter(|(r, _)| r["requestId"] == request.as_str() || r["code"] == request.as_str()).collect();
    let [(r, decide)] = matching.as_slice() else {
        bail!("No single request matches {request}. See `codync-host access list`.");
    };
    local_call(port, decide, json!({"requestId": r["requestId"], "approve": approve})).await?;
    println!("{}", if approve { "Approved." } else { "Denied." });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_code_reads_as_on_the_phone() {
        assert_eq!(spaced("165780"), "165 780");
        assert_eq!(spaced(""), "");
    }
}
