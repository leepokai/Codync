//! Compare-and-swap for replacing a bot's enabled skill selection after a slow import.

use crate::store::BotConfig;
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub(super) fn check(cfg: &BotConfig, patch: &Value) -> Result<()> {
    if let Some(expected) = patch.get("ifSkills") {
        let expected: Vec<String> =
            serde_json::from_value(expected.clone()).context("ifSkills must contain skill ids")?;
        ensure!(cfg.skills == expected, "The enabled skills changed; read the bot again before improving its skill");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hermes_skill_replacement_cannot_overwrite_a_new_selection() {
        let mut cfg: BotConfig =
            serde_json::from_value(json!({"id":"a","name":"A","backend":"hermes","skills":["original"]})).unwrap();
        let patch = json!({"ifSkills":["original"],"skills":["improved"]});
        check(&cfg, &patch).unwrap();
        cfg.skills.push("user-selected".into());
        assert!(check(&cfg, &patch).is_err());
        cfg.skills.clear();
        assert!(check(&cfg, &patch).is_err());
    }
}
