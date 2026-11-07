//! A connector's command line: the program and its arguments.

use anyhow::{Result, anyhow, bail};
use serde_json::Value;

/// A `command` line and an optional `args` list: the program, then its arguments.
/// Without `args` (or with `null`) the line is split like a shell would (quotes work).
pub(super) fn split_command(line: &str, args: Option<&Value>) -> Result<(Option<String>, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() {
        bail!("give a command");
    }
    if let Some(args) = args.filter(|a| !a.is_null()) {
        return Ok((Some(line.to_owned()), string_args(args)?));
    }
    let mut words = shlex::split(line).ok_or_else(|| anyhow!("the command has an unclosed quote"))?.into_iter();
    Ok((words.next(), words.collect()))
}

/// An `args` list, every item of it a string.
pub(super) fn string_args(args: &Value) -> Result<Vec<String>> {
    args.as_array()
        .and_then(|args| args.iter().map(|a| a.as_str().map(str::to_owned)).collect())
        .ok_or_else(|| anyhow!("`args` are strings"))
}

#[cfg(test)]
mod tests {
    use super::split_command;
    use crate::market::{add_custom_connector, connectors, import_connectors};
    use serde_json::json;

    #[test]
    fn a_line_splits_only_without_args() {
        let split = |args: Option<serde_json::Value>| split_command("npx \"my server\"", args.as_ref()).expect("split");
        let words = (Some("npx".to_owned()), vec!["my server".to_owned()]);
        assert_eq!(split(None), words);
        assert_eq!(split(Some(json!(null))), words, "null is the same as no args");
        let whole = (Some("npx \"my server\"".to_owned()), vec!["-y".to_owned()]);
        assert_eq!(split(Some(json!(["-y"]))), whole);
    }

    #[test]
    fn args_that_are_not_strings_are_refused() {
        for args in [json!(42), json!("-y"), json!(["--port", 8080])] {
            let err = split_command("node", Some(&args)).expect_err("refused").to_string();
            assert!(err.contains("`args` are strings"), "{err}");
        }
        assert!(split_command("  ", None).is_err());
    }

    #[tokio::test]
    async fn install_and_import_read_args_the_same_way() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        add_custom_connector(&store, &json!({"name": "One", "command": "npx server", "args": null}))
            .await
            .expect("null args");
        import_connectors(&store, r#"{"mcpServers": {"Two": {"command": "uvx tool", "args": null}}}"#)
            .await
            .expect("null args");
        let lines = |store: &crate::store::Store| -> Vec<(Option<String>, Vec<String>)> {
            connectors(store).expect("read").into_iter().map(|c| (c.command, c.args)).collect()
        };
        let split = vec![
            (Some("npx".to_owned()), vec!["server".to_owned()]),
            (Some("uvx".to_owned()), vec!["tool".to_owned()]),
        ];
        assert_eq!(lines(&store), split);

        let bad = json!({"name": "Three", "command": "node", "args": ["--port", 8080]});
        assert!(add_custom_connector(&store, &bad).await.is_err());
        let pasted = r#"{"mcpServers": {"Four": {"command": "node"}, "Five": {"command": "node", "args": [1]}}}"#;
        assert!(import_connectors(&store, pasted).await.is_err());
        assert_eq!(lines(&store), split, "a refused install or paste adds nothing");
    }
}
