//! `codync-host cloud`: the relay this computer uses, its registration token and account approval.

use crate::local_api::local_call;
use anyhow::Result;
use serde_json::json;

#[derive(clap::Args)]
#[allow(clippy::struct_excessive_bools, reason = "independent command-line flags")]
pub struct CloudArgs {
    #[arg(long, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
    #[arg(long, conflicts_with = "disable")]
    enable: bool,
    #[arg(long)]
    disable: bool,
    /// How devices on this computer's account get in: `code` (compare the 6-digit code,
    /// the default) or `auto` (no check: anyone who gets into the account, or controls
    /// the cloud, can then add a device).
    #[arg(long, value_parser = ["code", "auto"])]
    approval: Option<String>,
    /// Whether the cloud assistant (ChatGPT, Claude…) you connect to Codync through
    /// mcp.codync.dev may message this computer's bots: `on` or `off` (the default). Its calls
    /// reach this computer through the cloud unencrypted. `on` needs this computer in a Codync account.
    #[arg(long, value_parser = ["on", "off"])]
    assistants: Option<String>,
    /// Cloud base URL for this computer. `https://…`, or `http://127.0.0.1` / `localhost` for a relay on this machine.
    #[arg(long, conflicts_with = "clear_url")]
    url: Option<String>,
    /// Forget the saved URL and use this build's Codync cloud again.
    #[arg(long)]
    clear_url: bool,
    /// Ask for the private relay's registration token (typed without echo, or piped on stdin),
    /// so it stays out of the URL and shell history.
    #[arg(long, requires = "url")]
    token: bool,
}

/// `codync-host cloud`: shows or changes the cloud this computer uses.
pub async fn run(args: CloudArgs) -> Result<()> {
    let CloudArgs { port, enable, disable, approval, assistants, url, clear_url, token } = args;
    let mut status = None;
    if clear_url {
        status = Some(local_call(port, "setCloudUrl", json!({ "url": "" })).await?);
    } else if let Some(url) = url {
        let mut body = json!({ "url": url });
        if token {
            let token = crate::secret_prompt::read("Relay token: ")?;
            // An empty answer would clear the saved token; `--clear-url` is the way to forget it.
            if token.is_empty() {
                anyhow::bail!("No relay token was given.");
            }
            body["token"] = token.into();
        }
        status = Some(local_call(port, "setCloudUrl", body).await?);
    }
    // Before the account settings, so a refused one doesn't keep the cloud from turning on.
    if enable || disable {
        status = Some(local_call(port, "setCloud", json!({"enabled": !disable})).await?);
    }
    if let Some(approval) = approval {
        status = Some(local_call(port, "setApproval", json!({ "approval": approval })).await?);
    }
    if let Some(assistants) = assistants {
        status = Some(local_call(port, "setAssistants", json!({ "enabled": assistants == "on" })).await?);
    }
    let status = match status {
        Some(status) => status,
        None => local_call(port, "cloudStatus", json!({})).await?,
    };
    let text = |k: &str| status[k].as_str().unwrap_or("-").to_owned();
    let owner = status["owner"]["email"].as_str().or(status["owner"]["userId"].as_str()).unwrap_or("none");
    println!(
        "enabled:    {}\nurl:        {}\nrelay:      {}\ntoken:      {}\nregistered: {}\nconnected:  {}\naccount:    {owner}\napproval:   {}\nassistants: {}\nlast error: {}",
        status["enabled"],
        text("url"),
        text("relay"),
        if status["tokenSet"] == true { "saved" } else { "none" },
        status["registered"],
        status["connected"],
        text("approval"),
        if status["assistants"] == true { "on" } else { "off" },
        text("lastError"),
    );
    if let Some(hint) = account_hint(&status) {
        println!("\n{hint}");
    }
    Ok(())
}

/// How a computer with no desktop app gets into an account (docs/architecture/connections.md, headless computers).
fn account_hint(status: &serde_json::Value) -> Option<&'static str> {
    if !status["owner"].is_null() {
        return None;
    }
    Some(if status["enabled"] != true || status["relay"] == "private" {
        "Not in an account. Phones pair with `codync-host pair` and a QR code; joining an account needs the Codync cloud (`codync-host cloud --enable`)."
    } else {
        "Not in an account. Add it from a signed-in phone: run `codync-host pair` and scan the code, or type the short code it prints. Or, from a desktop app that reaches this computer over SSH, choose Add to account."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_computer_out_of_an_account_says_how_to_join_one() {
        assert_eq!(account_hint(&json!({"owner": {"userId": "u"}, "enabled": true})), None);
        assert!(
            account_hint(&json!({"owner": null, "enabled": true, "relay": "codync"}))
                .unwrap()
                .contains("signed-in phone")
        );
        assert!(account_hint(&json!({"owner": null, "enabled": false})).unwrap().contains("--enable"));
    }
}
