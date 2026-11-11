//! Explicit backend readiness, using the saved owner policy and existing connection.

use super::{Installation, settings};
use crate::{LockExt, hub::Hub, store::BotConfig};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

const UNAVAILABLE: &str = "Saved environment could not be checked. Verify its existing connection, profile, dependencies and terminal policy.";

fn revision_matches(saved: Option<&settings::Saved>, expected: &str) -> Result<()> {
    ensure!(
        saved.is_some_and(|value| value.revision == expected && value.config.enabled),
        "Hermes settings changed or are disabled; reopen and apply before checking"
    );
    Ok(())
}

fn connection_env(hub: &Hub, bot: &BotConfig) -> Result<Vec<(String, String)>> {
    let mut env = crate::agent::auth::launch_env(&hub.store, "hermes", Some(&bot.id))?;
    if let Some(account) = bot.account.as_deref() {
        for (key, value) in crate::agent::account::env(account, "hermes")? {
            env.retain(|(saved, _)| saved != &key);
            env.push((key, value));
        }
    }
    Ok(env)
}

async fn probe(hub: &Hub, bot: &BotConfig, saved: &settings::Saved) -> Result<Value> {
    let started = tokio::time::Instant::now();
    let installation = Installation::resolve(&saved.config).await?;
    let owner = installation.prepare_owner().await?;
    let env = connection_env(hub, bot)?;
    let mut command = tokio::process::Command::new(&installation.python);
    command
        .arg("-B")
        .arg(owner.with_file_name("environment_probe.py"))
        .arg("--hermes-root")
        .arg(&installation.root)
        .current_dir(&bot.cwd)
        .envs(env)
        .env("CODYNC_HERMES_SETTINGS", serde_json::to_string(&saved.config)?)
        .env("CODYNC_HERMES_PROJECT_ROOT", &bot.cwd)
        .env("CODYNC_BOT_ID", &bot.id)
        .env("CODYNC_HERMES_HEALTH_BUDGET", (55.0 - started.elapsed().as_secs_f64()).clamp(1.0, 50.0).to_string());
    // output::run owns the process group/Windows Job and kills only this probe's
    // descendants if the deadline, output cap, RPC cancellation or an error fires.
    let output = crate::agent::process::output::run(command, 32 * 1024).await?;
    ensure!(output["success"] == true, "Environment probe did not complete");
    let answer: Value = serde_json::from_str(output["stdout"].as_str().context("Missing environment result")?)?;
    ensure!(
        ["local", "docker", "ssh", "modal", "daytona", "vercel_sandbox", "unknown"]
            .contains(&answer["backend"].as_str().unwrap_or_default())
            && ["healthy", "unavailable"].contains(&answer["status"].as_str().unwrap_or_default())
            && answer["detail"].as_str().is_some_and(|text| text.len() <= 1024 && !text.chars().any(char::is_control)),
        "Invalid environment health result"
    );
    Ok(answer)
}

pub(crate) async fn api(hub: &Arc<Hub>, bot: &BotConfig, body: &Value) -> Result<Value> {
    let expected = body["ifRevision"].as_str().context("Apply Hermes settings before checking their environment")?;
    let saved = settings::load(&hub.store, &bot.id)?;
    revision_matches(saved.as_ref(), expected)?;
    let saved = saved.context("Apply Hermes settings first")?;
    let result = tokio::time::timeout(Duration::from_secs(60), probe(hub, bot, &saved)).await;

    // An old in-flight check must never label the newly applied route healthy.
    let _guard = hub.store.connector_lock.locked();
    revision_matches(settings::load(&hub.store, &bot.id)?.as_ref(), expected)?;
    let current = hub.store.bot(&bot.id)?.context("The Hermes bot was removed")?;
    ensure!(
        !current.deleted
            && current.config.backend == "hermes"
            && current.config.cwd == bot.cwd
            && current.config.account == bot.account,
        "The bot's workspace or connection changed; check the saved environment again"
    );
    let mut answer = match result {
        Ok(Ok(answer)) => answer,
        Ok(Err(_)) => json!({"backend":saved.config.terminal.backend,"status":"unavailable","detail":UNAVAILABLE}),
        Err(_) => {
            json!({"backend":saved.config.terminal.backend,"status":"unavailable","detail":"Environment health check exceeded 60 seconds; probe cleanup could not be confirmed. Check the native sandbox dashboard."})
        }
    };
    if answer["backend"] == "" {
        answer["backend"] = "unknown".into();
    }
    answer["revision"] = expected.into();
    answer["checkedAt"] = crate::store::now_ms().into();
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_checks_require_the_exact_enabled_saved_revision() {
        let mut saved = settings::Saved { revision: "new".into(), config: settings::Settings::default() };
        assert!(revision_matches(None, "new").is_err());
        assert!(revision_matches(Some(&saved), "new").is_err());
        saved.config.enabled = true;
        assert!(revision_matches(Some(&saved), "old").is_err());
        assert!(revision_matches(Some(&saved), "new").is_ok());
    }
}
