//! `codync-host pair` and `codync-host access`: letting phones and devices in from a terminal. A computer in no
//! account asks before a phone comes in, and whether to join its account (docs/plans/optional-sign-in.md): `pair`
//! answers in a terminal, `access approve` / `deny` anywhere (scripts, SSH without a terminal).

use crate::local_api::local_call;
use crate::remote::pairing_requests::Answer;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::io::IsTerminal as _;
use std::time::Duration;

#[derive(clap::Subcommand)]
pub enum AccessAction {
    /// Show pending requests (account devices asking for access, phones asking to pair) with the code each device
    /// shows (the default).
    List,
    /// Approve a request, by its id or by the 6-digit code shown on the device.
    Approve {
        request: String,
        /// Let a pairing phone in without joining its account: it connects only over Wi-Fi or Tailscale.
        #[arg(long)]
        phone_only: bool,
    },
    /// Deny a request.
    Deny { request: String },
}

const ASKS: &str = "This computer isn't in an account, so it asks before a phone comes in. Answer within 90 seconds \
                    with `codync-host access approve <code>` (the 6 digits the phone shows; add `--phone-only` to stay \
                    out of the phone's account), or in the Codync desktop app or the terminal UI.";
const WIFI_ONLY: &str = "Without it, this phone can connect only over Wi-Fi or Tailscale.";

pub async fn pair(port: u16, as_json: bool) -> Result<()> {
    let hello = local_call(port, "hello", json!({})).await?;
    let p = local_call(port, "pairing", json!({"shortCode": true})).await?;
    let status = local_call(port, "cloudStatus", json!({})).await?;
    let asks = status["owner"].is_null();
    if as_json {
        println!(
            "{}",
            json!({
                "name": hello["name"], "computerId": hello["computerId"], "signKey": hello["signKey"],
                "boxKey": hello["boxKey"], "urls": p["urls"], "cloud": status["url"],
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
    println!("Scan with the Codync app within 10 minutes, or open this link on the phone:\n{url}\n");
    if let Some(code) = p["shortCode"].as_str() {
        println!("Or type this code where the app's pairing screen asks for a link: {code}\n");
    }
    if let Some(tip) = reach_tip(&status, &p["urls"]) {
        println!("{tip}");
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
                "\n{} wants to use this computer.\nCheck the phone shows {}.",
                r["deviceName"].as_str().unwrap_or("A phone"),
                spaced(r["code"].as_str().unwrap_or_default()),
            );
            let answer = answer(r).await?;
            let answered = local_call(port, "decidePairing", answer.body(&r["requestId"]));
            if answered.await.is_err() {
                println!("The phone stopped waiting, or it was answered elsewhere.");
            } else if answer == Answer::Decline {
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

/// How far a phone gets from this computer, when that's less than anywhere: `None` with a relay phones may use, or an
/// address that reaches past this network.
fn reach_tip(status: &Value, urls: &Value) -> Option<&'static str> {
    let far = |u: &Value| {
        u.as_str().is_some_and(|u| u.contains("://100.") || u.contains(".ts.net") || u.starts_with("https://"))
    };
    let relay_is_on = !status["url"].is_null();
    if (relay_is_on && !status["owner"].is_null())
        || status["relay"] == "private"
        || urls.as_array().is_some_and(|u| u.iter().any(far))
    {
        return None;
    }
    Some(if status["canJoinAccount"] == true {
        "Tip: the phone reaches this computer on the same network only. Add it to your account, or use Tailscale, to reach it from anywhere."
    } else {
        "Tip: the phone reaches this computer on the same network only. Turn on the Codync cloud or Tailscale to reach it from anywhere."
    })
}

/// Asks in the terminal: whether the phone gets in, and whether this computer joins its account when it has the choice.
async fn answer(request: &Value) -> Result<Answer> {
    let (can_join, phone_only) = (request["canJoin"] == true, request["phoneOnly"] == true);
    let or_decline = |yes: bool, answer: Answer| if yes { answer } else { Answer::Decline };
    if !can_join {
        let prompt = "Let it in? It can connect only over Wi-Fi or Tailscale. [y/N] ";
        return Ok(or_decline(crate::prompt::ask_yes_no(prompt).await?, Answer::PhoneOnly));
    }
    let email = request["email"].as_str().unwrap_or("a Codync account");
    if let Some(removes) = removes_line(request) {
        println!("{removes}");
    }
    if !phone_only {
        let prompt = format!("Add this computer to {email}'s account and let the phone in? [y/N] ");
        return Ok(or_decline(crate::prompt::ask_yes_no(&prompt).await?, Answer::Join));
    }
    if !crate::prompt::ask_yes_no("Let it in? [y/N] ").await? {
        return Ok(Answer::Decline);
    }
    let prompt = format!("Add this computer to {email}'s account? {WIFI_ONLY} [y/N] ");
    Ok(if crate::prompt::ask_yes_no(&prompt).await? { Answer::Join } else { Answer::PhoneOnly })
}

/// The phones that lose access if this computer joins, as one line.
fn removes_line(request: &Value) -> Option<String> {
    let names: Vec<&str> = request["removes"].as_array()?.iter().filter_map(Value::as_str).collect();
    (!names.is_empty()).then(|| format!("These phones lose access and must pair again: {}.", names.join(", ")))
}

/// `123 456`, as the phone shows it.
fn spaced(code: &str) -> String {
    match code.split_at_checked(3) {
        Some((a, b)) if code.len() == 6 => format!("{a} {b}"),
        _ => code.to_owned(),
    }
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

/// What a pairing request adds to its line in `access list`.
fn pairing_note(request: &Value) -> &'static str {
    match (request["canJoin"] == true, request["phoneOnly"] == true) {
        (true, true) => "  pairing: the computer joins this account (or --phone-only)",
        (true, false) => "  pairing: the computer joins this account",
        (false, _) => "  pairing: Wi-Fi or Tailscale only",
    }
}

fn print_requests(requests: &[(Value, &str)]) {
    if requests.is_empty() {
        println!("No devices are asking for access.");
    }
    for (r, decide) in requests {
        println!(
            "{}  {} ({}, {})  code {}{}",
            r["requestId"].as_str().unwrap_or_default(),
            r["deviceName"].as_str().unwrap_or_default(),
            r["platform"].as_str().unwrap_or_default(),
            r["email"].as_str().unwrap_or("no email"),
            r["code"].as_str().unwrap_or("(waiting for the device)"),
            if *decide == "decidePairing" { pairing_note(r) } else { "" },
        );
        if let Some(removes) = removes_line(r) {
            println!("  {removes}");
        }
    }
}

pub async fn access(port: u16, action: AccessAction) -> Result<()> {
    let requests = requests(port).await?;
    let (request, approve, phone_only) = match action {
        AccessAction::List => {
            print_requests(&requests);
            return Ok(());
        }
        AccessAction::Approve { request, phone_only } => (request, true, phone_only),
        AccessAction::Deny { request } => (request, false, false),
    };
    // A 6-digit code picks the request showing it; it must be the only one.
    let matching: Vec<_> =
        requests.iter().filter(|(r, _)| r["requestId"] == request.as_str() || r["code"] == request.as_str()).collect();
    let [(r, decide)] = matching.as_slice() else {
        bail!("No single request matches {request}. See `codync-host access list`.");
    };
    let mut body = json!({"requestId": r["requestId"], "approve": approve});
    if phone_only {
        if *decide != "decidePairing" {
            bail!("--phone-only is for a phone that's pairing with this computer.");
        }
        body["join"] = json!(false);
    }
    local_call(port, decide, body).await?;
    println!("{}", if approve { "Approved." } else { "Denied." });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_names_the_phones_a_join_removes() {
        assert_eq!(removes_line(&json!({"removes": []})), None);
        assert_eq!(removes_line(&json!({})), None);
        let line = removes_line(&json!({"removes": ["Pixel", "iPhone"]})).unwrap();
        assert!(line.ends_with("must pair again: Pixel, iPhone."), "{line}");
    }

    #[test]
    fn the_list_says_what_a_pairing_answer_does() {
        assert!(pairing_note(&json!({"canJoin": true, "phoneOnly": true})).contains("--phone-only"));
        assert!(pairing_note(&json!({"canJoin": true, "phoneOnly": false})).ends_with("joins this account"));
        assert!(pairing_note(&json!({"canJoin": false, "phoneOnly": true})).contains("Wi-Fi or Tailscale only"));
    }

    #[test]
    fn the_tip_says_how_to_reach_further() {
        let urls = json!(["http://192.168.1.5:19222"]);
        let no_account =
            json!({"url": "https://api.codync.dev", "owner": null, "relay": "codync", "canJoinAccount": true});
        assert!(reach_tip(&no_account, &urls).unwrap().contains("Add it to your account"));
        let off = json!({"url": null, "owner": null, "relay": "codync", "canJoinAccount": false});
        assert!(reach_tip(&off, &urls).unwrap().contains("Turn on the Codync cloud"));
        let in_account = json!({"url": "https://api.codync.dev", "owner": {"userId": "u"}, "relay": "codync"});
        assert_eq!(reach_tip(&in_account, &urls), None);
        assert_eq!(reach_tip(&no_account, &json!(["http://100.101.1.1:19222"])), None, "Tailscale reaches it");
        let private = json!({"url": "https://relay.example.com", "owner": null, "relay": "private"});
        assert_eq!(reach_tip(&private, &urls), None, "a private relay is the user's own");
    }

    #[test]
    fn the_check_code_reads_as_on_the_phone() {
        assert_eq!(spaced("165780"), "165 780");
        assert_eq!(spaced(""), "");
    }
}
