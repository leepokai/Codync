//! The terminal's compact output editor is parsed by the host, never executed as a command.

use super::{Output, Routine, outputs};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub(super) fn request(body: &Value) -> Result<Option<Vec<Output>>> {
    ensure!(body.get("outputs").is_none() || body.get("outputText").is_none(), "Use outputs or outputText, not both");
    if let Some(text) = body.get("outputText") {
        return parse(text.as_str().context("outputText must be text")?).map(Some);
    }
    body.get("outputs").map(|value| outputs::parse(Some(value))).transpose()
}

pub(super) fn check_expected(body: &Value, saved: Option<&Routine>) -> Result<()> {
    if let Some(expected) = body.get("ifOutputs") {
        let expected: Vec<Output> =
            serde_json::from_value(expected.clone()).context("ifOutputs must be an output list")?;
        ensure!(
            expected.as_slice() == saved.map_or(&[][..], |routine| routine.outputs.as_slice()),
            "Routine outputs changed. Reopen the editor before replacing them"
        );
    }
    Ok(())
}

fn parse(text: &str) -> Result<Vec<Output>> {
    ensure!(
        text.len() <= 16 * 1024 && !text.contains('\0'),
        "Output text must be at most 16 KiB without NUL characters"
    );
    let words = shlex::split(text).context("Close every quoted output value")?;
    if words.len() == 1 && words[0] == "none" {
        return Ok(Vec::new());
    }
    let mut words = words.into_iter().peekable();
    let mut outputs = Vec::new();
    while let Some(kind) = words.next() {
        let target = words.next().context("Each output needs its kind and destination")?;
        let mut output = Output { kind, target, account: None, value: None };
        while let Some(key) = words.next_if(|word| word != ";") {
            let field = match key.as_str() {
                "from" => &mut output.account,
                "value" => &mut output.value,
                _ => anyhow::bail!("Use from <account> or value <text>, and separate outputs with ;"),
            };
            ensure!(field.is_none(), "An output field was repeated");
            *field = Some(words.next().context("An output field needs its value")?);
        }
        let _separator = words.next_if(|word| word == ";");
        if let Some(problem) = outputs::problem(&output) {
            anyhow::bail!("{problem}");
        }
        outputs.push(output);
        ensure!(outputs.len() <= outputs::MAX, "At most {} outputs", outputs::MAX);
    }
    Ok(outputs)
}

pub(super) fn format(outputs: &[Output]) -> Result<String> {
    let text = outputs
        .iter()
        .map(|output| {
            let mut words = vec![output.kind.as_str(), output.target.as_str()];
            if let Some(account) = &output.account {
                words.extend(["from", account.as_str()]);
            }
            if let Some(value) = &output.value {
                words.extend(["value", value.as_str()]);
            }
            words
                .into_iter()
                .map(|word| {
                    shlex::try_quote(word)
                        .map(std::borrow::Cow::into_owned)
                        .map_err(|_| anyhow::anyhow!("An output contains an unsupported NUL character"))
                })
                .collect::<Result<Vec<_>>>()
                .map(|words| words.join(" "))
        })
        .collect::<Result<Vec<_>>>()
        .map(|lines| lines.join(" ; "))?;
    ensure!(text.len() <= 16 * 1024, "Outputs are too large for the terminal text editor; edit them in an app");
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn terminal_outputs_round_trip_accounts_quotes_and_multiline_titles() {
        let outputs = vec![
            Output {
                kind: "whatsapp".into(),
                target: "5215555555555@s.whatsapp.net".into(),
                account: Some("5215555555556@s.whatsapp.net".into()),
                value: None,
            },
            Output { kind: "telegram".into(), target: "-100123".into(), account: Some("my bot".into()), value: None },
            Output {
                kind: "githubIssue".into(),
                target: "owner/repo".into(),
                account: None,
                value: Some("Today's \"report\"; line\nnext $(literal)".into()),
            },
        ];
        assert_eq!(parse(&format(&outputs).unwrap()).unwrap(), outputs);
    }

    #[test]
    fn terminal_outputs_reject_incomplete_or_conflicting_changes_and_clear_explicitly() {
        assert!(parse("whatsapp 5215555555555@s.whatsapp.net").is_err());
        assert!(parse("slack C1 from").is_err());
        assert!(parse("slack C1 value 'open").is_err());
        assert!(parse("slack C1 value a value b").is_err());
        assert!(parse("slack C1\0").is_err());
        assert_eq!(parse("none").unwrap().len(), 0);
        assert_eq!(parse("").unwrap().len(), 0);
        assert!(request(&json!({"outputs":[],"outputText":"none"})).is_err());
        assert!(request(&json!({})).unwrap().is_none());
    }

    #[test]
    fn unrelated_routine_edits_preserve_newer_destinations_and_stale_replacements_fail_atomically() {
        use crate::LockExt;
        use crate::hub::Hub;
        use crate::remote::identity::Identity;
        use crate::store::{BotConfig, Store};
        let root = std::env::temp_dir().join(format!("codync-output-edit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        let bot: BotConfig =
            serde_json::from_value(json!({"id":"bot","name":"Bot","backend":"fixture","cwd":root})).unwrap();
        hub.store.save_bot(&bot).unwrap();
        let mut routine = crate::routines::tests::routine();
        routine.outputs = vec![Output { kind: "slack".into(), target: "C-newer".into(), account: None, value: None }];
        hub.routines
            .change(&hub.store, |state| {
                state.routines.push(routine);
                Ok(())
            })
            .unwrap();
        let mut body = json!({"id":"routine","name":"Renamed","instruction":"Check status","triggers":[{"type":"interval","seconds":60}]});
        let saved = hub.routines.save(&hub, "bot", &body).unwrap();
        assert_eq!(
            saved["routine"]["outputs"][0]["target"], "C-newer",
            "omitting untouched outputs preserves another client's update"
        );
        body["name"] = "Stale replacement".into();
        body["outputs"] = json!([{"kind":"slack","target":"C-replacement"}]);
        body["ifOutputs"] = json!([{"kind":"slack","target":"C-original"}]);
        assert!(hub.routines.save(&hub, "bot", &body).unwrap_err().to_string().contains("outputs changed"));
        let state = hub.routines.state.locked();
        assert_eq!(state.routines[0].name, "Renamed");
        assert_eq!(state.routines[0].outputs[0].target, "C-newer");
        drop(state);
        body["ifOutputs"] = json!([{"kind":"slack","target":"C-newer"}]);
        assert!(hub.routines.save(&hub, "bot", &body).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
}
