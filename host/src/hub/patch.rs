//! An edit to a bot's settings: the fields a client sends, laid over what is saved.

use crate::store::BotConfig;
use anyhow::{Result, bail};
use serde_json::Value;

/// `stored` with `patch` laid over it: a field the patch leaves out keeps its saved value, a `null`
/// clears it. The id, creation and lead fields never change here.
pub(super) fn patched(stored: &BotConfig, patch: &Value) -> Result<BotConfig> {
    let mut merged = serde_json::to_value(stored)?;
    for (k, v) in patch.as_object().into_iter().flatten() {
        if !matches!(k.as_str(), "id" | "createdAt" | "createdBy" | "lead") {
            merged[k] = v.clone();
        }
    }
    let mut cfg: BotConfig = serde_json::from_value(merged)?;
    // A list that names each step's agent alone (a bot's own tools) sends no login profile, which isn't the user
    // clearing it: the step keeps its saved one (the apps send `loginProfile`, `null` for none).
    if let Some(sent) = patch.get("fallbacks").and_then(Value::as_array) {
        let agents: Vec<&str> = sent.iter().map(|step| step["backend"].as_str().unwrap_or_default()).collect();
        let saved = crate::agent::fallback::saved_steps(&stored.fallbacks, &agents);
        for ((step, sent), saved) in cfg.fallbacks.iter_mut().zip(sent).zip(saved) {
            if sent.get("loginProfile").is_none() {
                step.login_profile = saved.and_then(|f| f.login_profile.clone());
            }
        }
    }
    if cfg.name.trim().is_empty() {
        cfg.name.clone_from(&stored.name);
    }
    // A name the user picked ends automatic naming; the namer passes `autoName` itself.
    if cfg.name != stored.name && patch.get("autoName").is_none() {
        cfg.auto_name = false;
    }
    // Options belong to one model; an app that doesn't know them can't clear them itself.
    if (cfg.model != stored.model || cfg.backend != stored.backend) && patch.get("modelOptions").is_none() {
        cfg.model_options.clear();
    }
    // So do modes, to one agent: on another, a picked mode becomes its Auto mode, as the apps do it.
    if cfg.backend != stored.backend
        && patch.get("mode").is_none()
        && cfg.mode.as_deref().is_some_and(|m| m != crate::agent::modes::AUTO)
    {
        cfg.mode = Some(crate::agent::modes::AUTO.into());
    }
    if cfg.kind != stored.kind {
        bail!("a bot can't become a group, or a group a bot");
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::patched;
    use crate::store::BotConfig;
    use serde_json::json;

    #[test]
    fn a_patch_keeps_the_saved_section_unless_it_names_one() {
        let stored: BotConfig = serde_json::from_value(
            json!({"id": "a", "name": "a", "backend": "claude", "cwd": "/tmp", "section": "Play"}),
        )
        .expect("a bot config");
        let section = |patch| patched(&stored, &patch).expect("a valid patch").section;
        assert_eq!(section(json!({"id": "a", "name": "b"})).as_deref(), Some("Play"), "left out: kept");
        assert_eq!(section(json!({"id": "a", "section": null})), None, "null: out of its section");
        assert_eq!(section(json!({"id": "a", "section": "Work"})).as_deref(), Some("Work"));
    }

    #[test]
    fn a_fallback_sent_without_a_login_profile_keeps_its_saved_one() {
        let stored: BotConfig =
            serde_json::from_value(json!({"id": "a", "name": "a", "backend": "codex", "fallbacks": [
                {"backend": "claude", "loginProfile": "a1"}, {"backend": "claude", "loginProfile": "a2"},
            ]}))
            .expect("a bot config");
        let profiles = |fallbacks| -> Vec<Option<String>> {
            let cfg = patched(&stored, &json!({"name": "b", "fallbacks": fallbacks})).expect("a valid patch");
            cfg.fallbacks.into_iter().map(|f| f.login_profile).collect()
        };
        let older = json!([{"backend": "claude"}, {"backend": "gemini"}, {"backend": "claude", "model": "opus"}]);
        assert_eq!(profiles(older), [Some("a1".into()), None, Some("a2".into())], "a list of agents alone");
        let cleared = json!([{"backend": "claude", "loginProfile": null}, {"backend": "claude", "loginProfile": "a1"}]);
        assert_eq!(profiles(cleared), [None, Some("a1".into())], "a step that names its login profile, or none");
    }

    #[test]
    fn another_agent_works_in_its_auto_mode_unless_the_patch_picks_one() {
        let stored: BotConfig =
            serde_json::from_value(json!({"id": "a", "name": "a", "backend": "claude", "cwd": "/tmp", "mode": "plan"}))
                .expect("a bot config");
        let mode = |patch| patched(&stored, &patch).expect("a valid patch").mode;
        assert_eq!(mode(json!({"backend": "codex"})).as_deref(), Some("auto"), "plan is Claude's");
        assert_eq!(mode(json!({"backend": "codex", "mode": "read-only"})).as_deref(), Some("read-only"));
        assert_eq!(mode(json!({"name": "b"})).as_deref(), Some("plan"));
    }
}
