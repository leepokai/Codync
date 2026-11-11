//! Quoted route rows are data, never executable shell commands.
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub(super) fn display(routes: &Value) -> String {
    routes
        .as_array()
        .into_iter()
        .flatten()
        .map(|route| {
            ["label", "provider", "model"]
                .map(|key| format!("'{}'", route[key].as_str().unwrap_or_default().replace('\'', "'\\''")))
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn parse(text: &str, previous: &Value) -> Result<Value> {
    let mut routes = Vec::new();
    let mut reused = std::collections::HashSet::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let parts = shlex::split(line).ok_or_else(|| anyhow::anyhow!("Close each route's quotes"))?;
        ensure!(parts.len() == 3, "Each route needs three quoted values: name provider model");
        ensure!(routes.len() < 16, "Save at most 16 routes");
        let id = previous
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|route| {
                let id = route["id"].as_str()?;
                (["label", "provider", "model"]
                    .iter()
                    .zip(&parts)
                    .all(|(key, value)| route[*key].as_str() == Some(value.as_str()))
                    && !reused.contains(id))
                .then(|| id.to_owned())
            })
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        reused.insert(id.clone());
        routes.push(json!({"id":id,"label":parts[0],"provider":parts[1],"model":parts[2]}));
    }
    Ok(routes.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_other_fields_preserves_route_ids_and_quotes() {
        let routes = json!([{"id":"stable","label":"Ana's investigación","provider":"","model":"a b"}]);
        assert_eq!(parse(&display(&routes), &routes).unwrap(), routes);
        assert!(parse("'unclosed", &routes).is_err());
        assert!(parse("a b c d", &routes).is_err());
    }
}
