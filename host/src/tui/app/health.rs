//! The computer's health in the terminal: one word in the header when memory is short, from `hostHealth`.

use serde_json::Value;

/// How often the header looks at the computer again.
pub const REFRESH: std::time::Duration = std::time::Duration::from_secs(15);

/// What the header says: nothing while all is well, else why a person should care (same words as the other clients).
pub fn alert(health: &Value) -> Option<String> {
    let used = health["ramUsedPercent"].as_u64()?;
    match health["level"].as_str()? {
        "critical" if health["refusing"] == true => Some(format!("Memory {used} % · not starting new turns")),
        "critical" | "warning" => Some(format!("Memory {used} % · getting full")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_header_is_silent_until_memory_is_short_and_then_says_why() {
        assert_eq!(alert(&Value::Null), None, "an older host says nothing");
        assert_eq!(alert(&json!({"level": "normal", "ramUsedPercent": 40})), None);
        assert_eq!(
            alert(&json!({"level": "warning", "ramUsedPercent": 75})),
            Some("Memory 75 % · getting full".into())
        );
        assert_eq!(
            alert(&json!({"level": "critical", "refusing": true, "ramUsedPercent": 90})),
            Some("Memory 90 % · not starting new turns".into())
        );
    }
}
