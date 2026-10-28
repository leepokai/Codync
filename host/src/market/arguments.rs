//! Registry arguments and the value templates a connector is installed with: the inputs a
//! package install asks for, the command-line words it runs with, and the credentials its
//! templates name.

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The inputs a registry argument list asks the user for.
pub(super) fn inputs(list: &Value) -> Vec<Value> {
    list.as_array().into_iter().flatten().flat_map(argument_inputs).collect()
}

/// The inputs a registry argument asks the user for: a named option's value, a
/// positional's hint, or the variables its value template names.
fn argument_inputs(a: &Value) -> Vec<Value> {
    if let Some(variables) = a["variables"].as_object().filter(|variables| !variables.is_empty()) {
        let required = a["isRequired"].as_bool().unwrap_or(false);
        return variables
            .iter()
            .filter(|(_, variable)| !variable["value"].is_string())
            .map(|(name, variable)| {
                let description = variable.get("description").unwrap_or(&a["description"]);
                json!({"name": name, "description": description,
                       "secret": variable["isSecret"].as_bool().unwrap_or(false),
                       "required": required || variable["isRequired"].as_bool().unwrap_or(false),
                       "default": variable["default"]})
            })
            .collect();
    }
    argument_input(a)
        .into_iter()
        .map(|key| {
            json!({"name": key, "description": a["description"], "secret": a["isSecret"].as_bool().unwrap_or(false),
                   "required": a["isRequired"].as_bool().unwrap_or(false), "default": a["default"],
                   "placeholder": a["valueHint"]})
        })
        .collect()
}

/// The input a registry argument asks the user for: a positional's hint, or a named
/// option's name. None when the registry fixes it (a set value, or a bare flag).
fn argument_input(a: &Value) -> Option<String> {
    if a["value"].is_string() {
        return None;
    }
    let text = |k: &str| a[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    match a["type"].as_str() {
        Some("named") => text("name").filter(|_| a["valueHint"].is_string() || a["default"].is_string()),
        _ => text("valueHint").or_else(|| text("name")).or_else(|| Some("value".into())),
    }
}

/// The command-line words a registry argument list makes, plus the values its templates named.
pub(super) struct Arguments {
    pub words: Vec<Vec<String>>,
    pub values: BTreeMap<String, String>,
    pub templates: BTreeMap<usize, BTreeMap<String, Option<String>>>,
}

/// A registry argument list as command-line words, taking values from the user's inputs.
/// The values a template named come back too: they stay with the connector as credentials,
/// and the template is filled in when the connector launches.
pub(super) fn words(list: &Value, input: &dyn Fn(&str) -> Option<String>) -> Result<Arguments> {
    let mut out = Arguments { words: vec![], values: BTreeMap::new(), templates: BTreeMap::new() };
    let mut offset = 0;
    for a in list.as_array().into_iter().flatten() {
        let Some((argument, values, template)) = argument_words(a, input)? else { continue };
        if !template.is_empty() {
            out.templates.insert(offset + argument.len() - 1, template);
        }
        offset += argument.len();
        out.words.push(argument);
        out.values.extend(values);
    }
    Ok(out)
}

/// One registry argument's command-line words and the values its template named.
type ArgumentWords = (Vec<String>, BTreeMap<String, String>, BTreeMap<String, Option<String>>);

/// One registry argument as command-line words and the values its template named. None when
/// an optional variable is empty: the registry means the whole argument to be left out.
fn argument_words(a: &Value, input: &dyn Fn(&str) -> Option<String>) -> Result<Option<ArgumentWords>> {
    let mut words = vec![];
    let mut template = BTreeMap::new();
    let values = if let Some(variables) = a["variables"].as_object().filter(|variables| !variables.is_empty()) {
        let Some(mut values) = template_values(a, variables, input)? else { return Ok(None) };
        for name in values.keys() {
            template.insert(name.clone(), variables[name]["value"].as_str().map(str::to_owned));
        }
        values.retain(|name, _| !variables[name]["value"].is_string());
        // The argument keeps its template; the credential is filled in at launch.
        words.push(a["value"].as_str().unwrap_or_default().to_owned());
        values
    } else {
        let value = if let Some(key) = argument_input(a) {
            match input(&key).or_else(|| a["default"].as_str().map(str::to_owned)) {
                Some(value) => Some(value),
                None if a["isRequired"].as_bool().unwrap_or(false) => bail!("{key} is required"),
                None => return Ok(None),
            }
        } else {
            a["value"].as_str().map(str::to_owned)
        };
        words.extend(value);
        BTreeMap::new()
    };
    if a["type"] == "named"
        && let Some(name) = a["name"].as_str().filter(|name| !name.is_empty())
    {
        words.insert(0, name.to_owned());
    }
    Ok((!words.is_empty()).then_some((words, values, template)))
}

/// The `{name}` parts of a value template.
fn placeholders(template: &str) -> Vec<&str> {
    template.split('{').skip(1).filter_map(|part| part.split_once('}').map(|(name, _)| name)).collect()
}

/// Replaces each `{name}` with its value; a name without a value stays as written.
pub(super) fn fill_template(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut filled = String::new();
    let mut rest = template;
    while let Some((head, tail)) = rest.split_once('{') {
        let Some((name, remaining)) = tail.split_once('}') else { break };
        filled.push_str(head);
        if let Some(value) = values.get(name) {
            filled.push_str(value);
        } else {
            filled.push('{');
            filled.push_str(name);
            filled.push('}');
        }
        rest = remaining;
    }
    filled.push_str(rest);
    filled
}

/// Legacy installs saved registry templates as literal words without tracking their variables.
pub(super) fn has_legacy_templates(words: &[String], package: &Value) -> bool {
    ["runtimeArguments", "packageArguments"].iter().flat_map(|key| package[*key].as_array().into_iter().flatten()).any(
        |argument| {
            argument["variables"].as_object().is_some_and(|variables| !variables.is_empty())
                && argument["value"].as_str().is_some_and(|template| words.iter().any(|word| word == template))
        },
    )
}

/// Reads the values a value template names, so the connector can keep them as credentials
/// and fill the template in when it launches. None when an optional variable is empty: the
/// registry means the whole argument to be left out.
fn template_values(
    argument: &Value,
    variables: &serde_json::Map<String, Value>,
    input: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<BTreeMap<String, String>>> {
    let template = argument["value"].as_str().unwrap_or_default();
    let required = argument["isRequired"].as_bool().unwrap_or(false);
    let mut values = BTreeMap::new();
    for name in placeholders(template) {
        let Some(variable) = variables.get(name) else {
            bail!("this argument has parts the registry doesn't describe; add it as a custom connector");
        };
        let Some(filled) = variable["value"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| input(name))
            .or_else(|| variable["default"].as_str().map(str::to_owned))
        else {
            if required || variable["isRequired"].as_bool().unwrap_or(false) {
                bail!("{name} is required");
            }
            return Ok(None);
        };
        values.insert(name.to_owned(), filled);
    }
    Ok(Some(values))
}

/// Puts the user's values into a URL's `{name}` parts, or their defaults.
pub(super) fn fill_url(url: &str, variables: &Value, input: &dyn Fn(&str) -> Option<String>) -> Result<String> {
    let mut values = BTreeMap::new();
    for name in placeholders(url) {
        let Some(variable) = variables.get(name) else {
            bail!("this server's URL has parts the registry doesn't describe; add it as a custom connector");
        };
        let value = variable["value"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| input(name))
            .or_else(|| variable["default"].as_str().map(str::to_owned))
            .ok_or_else(|| anyhow!("{name} is required"))?;
        values.insert(name.to_owned(), value);
    }
    let filled = fill_template(url, &values);
    if filled.contains('{') {
        bail!("this server's URL has unresolved parts; add it as a custom connector");
    }
    Ok(filled)
}

/// Fills the `{placeholder}` in a header template with the user's value, or uses it as-is.
pub(super) fn fill_header(template: Option<&str>, value: &str) -> String {
    match template {
        Some(template) if template.contains('{') && template.contains('}') => {
            let (head, rest) = template.split_once('{').unwrap_or((template, ""));
            let tail = rest.split_once('}').map_or("", |(_, tail)| tail);
            format!("{head}{value}{tail}")
        }
        _ => value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_arguments_become_words() {
        let list = json!([
            {"type": "named", "name": "--rm"},
            {"type": "named", "name": "--db", "valueHint": "path", "isRequired": true},
            {"type": "named", "name": "--mode", "value": "ro"},
            {"type": "positional", "valueHint": "source", "isRequired": false},
        ]);
        let input = |k: &str| (k == "--db").then(|| "/tmp/a.db".to_owned());
        let parsed = words(&list, &input).unwrap();
        assert_eq!(
            parsed.words,
            [
                vec!["--rm".to_owned()],
                vec!["--db".to_owned(), "/tmp/a.db".to_owned()],
                vec!["--mode".to_owned(), "ro".to_owned()]
            ]
        );
        assert!(parsed.values.is_empty(), "nothing was named by a template");
        assert!(words(&list, &|_: &str| None).is_err(), "--db is required");
        assert_eq!(argument_input(&list[0]), None);
        assert_eq!(argument_input(&list[3]).as_deref(), Some("source"));
    }

    #[test]
    fn argument_variables_become_credentials_and_empty_ones_are_left_out() {
        let list = json!([
            {"type": "named", "name": "-e", "value": "GITHUB_OAUTH_CALLBACK_PORT=8085"},
            {"type": "named", "name": "-e", "value": "GITHUB_PERSONAL_ACCESS_TOKEN={token}",
             "description": "Optional GitHub Personal Access Token. Omit to log in with OAuth on first use.",
             "variables": {"token": {"format": "string", "isSecret": true}}},
        ]);
        let input = |k: &str| (k == "token").then(|| "ghp_x".to_owned());
        let parsed = words(&list, &input).unwrap();
        assert_eq!(
            parsed.words,
            [
                vec!["-e".to_owned(), "GITHUB_OAUTH_CALLBACK_PORT=8085".to_owned()],
                // The word keeps its template; the credential is filled in at launch.
                vec!["-e".to_owned(), "GITHUB_PERSONAL_ACCESS_TOKEN={token}".to_owned()]
            ]
        );
        assert_eq!(parsed.values["token"], "ghp_x");
        let parsed = words(&list, &|_: &str| None).unwrap();
        assert_eq!(parsed.words, [vec!["-e".to_owned(), "GITHUB_OAUTH_CALLBACK_PORT=8085".to_owned()]]);
        assert!(parsed.values.is_empty(), "an optional variable left empty adds no credential");
        let inputs = argument_inputs(&list[1]);
        assert_eq!(
            (inputs[0]["name"].as_str(), inputs[0]["secret"].as_bool(), inputs[0]["required"].as_bool()),
            (Some("token"), Some(true), Some(false))
        );
        assert_eq!(
            inputs[0]["description"].as_str(),
            Some("Optional GitHub Personal Access Token. Omit to log in with OAuth on first use.")
        );
        let required = json!([{"type": "named", "name": "-e", "value": "KEY={token}", "variables": {"token": {"isRequired": true}}}]);
        assert!(words(&required, &|_: &str| None).is_err(), "a required variable must be filled");
    }

    #[test]
    fn a_template_part_the_registry_does_not_describe_is_refused() {
        let list = json!([{"type": "named", "name": "-e", "value": "KEY={token}", "variables": {"other": {}}}]);
        assert!(words(&list, &|_: &str| None).is_err());
        assert!(fill_url("https://x/{agent}/mcp", &json!({"other": {}}), &|_: &str| None).is_err());
    }

    #[test]
    fn substituted_values_are_literal_even_when_they_contain_placeholders() {
        let values = BTreeMap::from([("a".into(), "literal{z}".into()), ("z".into(), "other".into())]);
        assert_eq!(
            fill_template("KEY={a}/{z}/{a}/{unknown}/{unfinished", &values),
            "KEY=literal{z}/other/literal{z}/{unknown}/{unfinished"
        );
    }

    #[test]
    fn fixed_variables_are_not_inputs_and_cannot_be_overridden() {
        let list = json!([{"type": "named", "name": "-e", "value": "PORT={port}",
                           "variables": {"port": {"value": "8085", "default": "9999"}}}]);
        assert_eq!(inputs(&list), Vec::<Value>::new());
        let parsed = words(&list, &|_| Some("1234".into())).unwrap();
        assert_eq!(parsed.templates[&1]["port"].as_deref(), Some("8085"));
        assert_eq!(parsed.values, BTreeMap::new());
        assert_eq!(words(&list, &|_| None).unwrap().templates[&1]["port"].as_deref(), Some("8085"));
        let vars = json!({"region": {"value": "us"}});
        assert_eq!(
            fill_url("https://{region}.example/mcp", &vars, &|_| Some("other".into())).unwrap(),
            "https://us.example/mcp"
        );
    }

    #[test]
    fn url_templates_fill_in() {
        let vars = json!({"agent_id": {"isRequired": true}, "region": {"default": "us"}});
        let input = |k: &str| (k == "agent_id").then(|| "a1".to_owned());
        assert_eq!(fill_url("https://x/{region}/{agent_id}/mcp", &vars, &input).unwrap(), "https://x/us/a1/mcp");
        assert!(fill_url("https://x/{agent_id}", &vars, &|_: &str| None).is_err());
        assert!(fill_url("https://x/{other}", &json!({}), &|_: &str| None).is_err());
        assert!(fill_url("https://x/{unfinished", &json!({}), &|_| None).is_err());
    }

    #[test]
    fn header_templates_are_filled() {
        assert_eq!(fill_header(Some("Bearer {api_key}"), "abc"), "Bearer abc");
        assert_eq!(fill_header(Some("token"), "abc"), "abc");
        assert_eq!(fill_header(None, "abc"), "abc");
    }
}
