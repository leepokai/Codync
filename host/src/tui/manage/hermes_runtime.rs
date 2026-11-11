//! One Apply for Hermes's host-owned runtime configuration. Frozen revisions prevent lost edits.

use super::{Editor, Fields, Input, Submit};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};

const BACKENDS: [(&str, &str); 7] = [
    ("", "Native profile"),
    ("local", "Local"),
    ("docker", "Docker"),
    ("ssh", "SSH"),
    ("modal", "Modal"),
    ("daytona", "Daytona"),
    ("vercel_sandbox", "Vercel Sandbox"),
];

fn input(name: &str, label: &str, value: &Value, placeholder: &str) -> Input {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    };
    Input {
        name: name.into(),
        label: label.into(),
        secret: false,
        required: name == "enabled",
        multiline: false,
        placeholder: placeholder.into(),
        ed: Editor::with(&text),
    }
}

pub(super) fn fields(bot: &str, reply: &Value) -> Fields {
    let config = reply["config"].as_object().map_or_else(|| json!({"enabled":false}), |c| Value::Object(c.clone()));
    let terminal = &config["terminal"];
    let delegation = &config["delegation"];
    let backend = terminal["backend"].as_str().unwrap_or_default();
    let available = reply["installation"]["available"].as_bool();
    let installation = match available {
        Some(true) => reply["installation"]["profileSummary"]
            .as_str()
            .unwrap_or("Pinned installation found; remote health has not been checked."),
        Some(false) => "Pinned installation not found; set its root and Python paths.",
        None => "Installation and remote health have not been checked.",
    };
    let mut inputs = vec![input(
        "enabled",
        "Owned runtime (true/false)",
        &json!(config["enabled"].as_bool().unwrap_or(false)),
        "false",
    )];
    for (name, label, placeholder) in [
        ("ssh_host", "SSH host or config alias", "existing SSH connection"),
        ("ssh_user", "SSH user", "remote user"),
        ("ssh_port", "SSH port", "blank: native default"),
        ("ssh_key", "SSH key path", "blank: SSH config/agent"),
        ("docker_image", "Docker image", "blank: native profile"),
        ("modal_mode", "Modal mode", "blank / auto / managed / direct"),
        ("vercel_runtime", "Vercel runtime", "blank / node24 / node22 / python3.13"),
    ] {
        inputs.push(input(name, label, &terminal[name], placeholder));
    }
    inputs.push(input("memoryProvider", "Memory provider", &config["memoryProvider"], "native / builtin / honcho"));
    for (name, label, placeholder) in [
        ("provider", "Subagent provider", "blank: inherit native route"),
        ("model", "Subagent model", "blank: inherit native route"),
        ("max_iterations", "Iterations per subagent", "blank: native / 1–200"),
        ("max_concurrent_children", "Concurrent subagents", "blank: native / 1–10"),
        ("child_timeout_seconds", "Subagent inactivity (seconds)", "blank: native / 30–3600"),
    ] {
        inputs.push(input(name, label, &delegation[name], placeholder));
    }
    inputs.push(input("root", "Pinned Hermes installation root", &config["root"], "blank: detect installation"));
    inputs.push(input("python", "Hermes environment Python", &config["python"], "blank: detect native venv"));
    let mut routes = input(
        "routes",
        "Saved task routes",
        &json!(super::hermes_routes::display(&delegation["routes"])),
        "One row: 'name' 'provider' 'model'; blank provider/model inherits",
    );
    routes.multiline = true;
    inputs.push(routes);
    Fields {
        title: "Hermes runtime".into(),
        note: format!(
            "Ctrl-P: check saved environment (may start sandbox). Enter: Apply after this turn. Inactivity is not a total deadline. {installation}"
        ),
        choices: BACKENDS.iter().map(|(_, label)| (*label).into()).collect(),
        choice: BACKENDS.iter().position(|(value, _)| *value == backend).unwrap_or(0),
        inputs,
        cursor: 0,
        error: (!BACKENDS.iter().any(|(value, _)| *value == backend))
            .then(|| "Unknown saved backend; reopen with a compatible host before applying.".into()),
        saving: false,
        submit: Submit::HermesRuntime { bot: bot.into(), revision: reply["revision"].clone(), config },
    }
}

fn text<'a>(values: &'a Map<String, Value>, name: &str) -> Result<&'a str> {
    match values.get(name) {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(value)) => Ok(value.trim()),
        _ => bail!("{name} must be text"),
    }
}

fn number(values: &Map<String, Value>, name: &str, minimum: u32, maximum: u32) -> Result<Value> {
    let raw = text(values, name)?;
    if raw.is_empty() {
        return Ok(Value::Null);
    }
    ensure!(raw.bytes().all(|byte| byte.is_ascii_digit()), "{name} must be a whole number");
    let value: u32 = raw.parse().with_context(|| format!("{name} must be {minimum}–{maximum}"))?;
    ensure!((minimum..=maximum).contains(&value), "{name} must be {minimum}–{maximum}");
    Ok(value.into())
}

pub(super) fn request(f: &Fields, values: &Map<String, Value>) -> Result<Value> {
    let Submit::HermesRuntime { bot, revision, config } = &f.submit else {
        bail!("This is not a Hermes runtime form");
    };
    let mut config = config.clone();
    ensure!(config.is_object(), "Invalid Hermes settings; reopen before applying");
    ensure!(
        BACKENDS.iter().any(|(value, _)| *value == config["terminal"]["backend"].as_str().unwrap_or_default()),
        "Unknown saved backend; reopen with a compatible host"
    );
    let backend = BACKENDS.get(f.choice).context("Choose a Hermes environment")?.0;
    if !config["terminal"].is_object() {
        config["terminal"] = json!({});
    }
    if !config["delegation"].is_object() {
        config["delegation"] = json!({});
    }
    config["enabled"] = match text(values, "enabled")? {
        "true" => true.into(),
        "false" => false.into(),
        _ => bail!("Owned runtime must be true or false"),
    };
    config["terminal"]["backend"] = backend.into();
    for name in ["ssh_host", "ssh_user", "ssh_key", "docker_image", "modal_mode", "vercel_runtime"] {
        config["terminal"][name] = text(values, name)?.into();
    }
    config["terminal"]["ssh_port"] = number(values, "ssh_port", 1, 65535)?;
    let memory = text(values, "memoryProvider")?;
    ensure!(["", "native", "builtin", "honcho"].contains(&memory), "Memory provider must be native, builtin or honcho");
    config["memoryProvider"] = if memory.is_empty() { "native" } else { memory }.into();
    for name in ["provider", "model"] {
        config["delegation"][name] = text(values, name)?.into();
    }
    for (name, minimum, maximum) in
        [("max_iterations", 1, 200), ("max_concurrent_children", 1, 10), ("child_timeout_seconds", 30, 3600)]
    {
        config["delegation"][name] = number(values, name, minimum, maximum)?;
    }
    for name in ["root", "python"] {
        config[name] = text(values, name)?.into();
    }
    config["delegation"]["routes"] =
        super::hermes_routes::parse(text(values, "routes")?, &config["delegation"]["routes"])?;
    Ok(json!({"botId":bot,"config":config,"ifRevision":revision}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(f: &Fields) -> Map<String, Value> {
        f.inputs.iter().map(|input| (input.name.clone(), Value::String(input.ed.text.clone()))).collect()
    }

    #[test]
    fn one_apply_uses_the_opening_revision_and_keeps_native_as_an_empty_backend() {
        let mut reply = json!({"revision":"opening","config":{"enabled":true,"memoryProvider":"native","terminal":{"backend":""},"delegation":{}}});
        let f = fields("bot", &reply);
        reply["revision"] = "newer".into();
        let mut inputs = values(&f);
        inputs.insert("max_iterations".into(), "25".into());
        let body = request(&f, &inputs).unwrap();
        assert_eq!(body["ifRevision"], "opening");
        assert_eq!(body["config"]["terminal"]["backend"], "");
        assert_eq!(body["config"]["delegation"]["max_iterations"], 25);
        assert!(body["config"]["terminal"]["ssh_port"].is_null());
    }

    #[test]
    fn apply_refuses_ambiguous_booleans_and_out_of_range_or_fractional_limits() {
        let f = fields("bot", &json!({"revision":null,"config":{"enabled":false}}));
        for (name, value) in [
            ("enabled", "yes"),
            ("max_iterations", "0"),
            ("max_concurrent_children", "11"),
            ("child_timeout_seconds", "29"),
            ("ssh_port", "1.5"),
        ] {
            let mut inputs = values(&f);
            inputs.insert(name.into(), value.into());
            assert!(request(&f, &inputs).is_err(), "{name}");
        }
    }
}
