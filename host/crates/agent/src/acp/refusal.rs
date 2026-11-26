//! A prompt that ends in a `refusal` after the adapter said it hit a quota can be a usage limit, not a decline:
//! Antigravity writes `Usage Limit Reached` into the chat as regular text, then answers `stopReason: "refusal"`
//! with no error object. Turn that one into the RPC error the fallback chain already understands. A genuine
//! content-policy refusal has none of these words and stays a refusal.

use super::{INTERNAL_ERROR, RpcError};
use serde_json::Value;

/// The words an adapter uses for an exhausted quota or a rate limit in the text it streams to the user.
const USAGE_TEXT: [&str; 7] = [
    "usage limit reached",
    "you have reached your current quota",
    "quota exhausted",
    "quota exceeded",
    "rate limit exceeded",
    "too many requests",
    "out of quota",
];

/// The text the adapter wrote into this turn, from its `session/update` text chunks.
#[derive(Default)]
pub struct PromptText(String);

impl PromptText {
    /// Collects a streamed text chunk.
    pub fn push(&mut self, text: &str) {
        self.0.push_str(text);
        self.0.push('\n');
    }

    /// The words the turn said, lowercased once for a case-insensitive match.
    fn normalized(&self) -> String {
        self.0.to_lowercase()
    }

    /// Whether the streamed text says the account hit a quota or rate limit.
    pub fn says_usage_limit(&self) -> bool {
        let text = self.normalized();
        USAGE_TEXT.iter().any(|word| text.contains(word))
    }
}

/// A refusal whose streamed text says the account hit a quota, as the error adapters otherwise return for a
/// usage limit. `None` for a refusal about anything else, which stays a plain refusal.
pub fn prompt_result(result: &Value, text: &PromptText) -> Option<anyhow::Error> {
    (result["stopReason"] == "refusal" && text.says_usage_limit()).then(|| {
        RpcError { code: INTERNAL_ERROR, message: text.0.trim().to_owned(), kind: Some("usage_limit_exceeded".into()) }
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn refusal() -> Value {
        json!({"stopReason": "refusal"})
    }

    fn error_of(result: &Value, text: &PromptText) -> anyhow::Error {
        prompt_result(result, text).expect("a usage limit in a refusal")
    }

    #[test]
    fn antigravitys_quota_message_in_a_refusal_is_a_usage_limit_error() {
        let mut text = PromptText::default();
        text.push("Usage Limit Reached");
        text.push(
            "\nYou have reached your current quota for this period. Your limit will reset in 1 minute, 20 seconds.",
        );
        let error = error_of(&refusal(), &text);
        let rpc = error.downcast_ref::<RpcError>().expect("an RpcError");
        assert_eq!((rpc.code, rpc.kind.as_deref()), (INTERNAL_ERROR, Some("usage_limit_exceeded")));
        assert!(rpc.message.contains("You have reached your current quota"));
    }

    #[test]
    fn a_quota_spoken_in_caps_or_across_chunks_is_recognized() {
        let mut text = PromptText::default();
        text.push("USAGE LIMIT");
        text.push(" REACHED — 429 TOO MANY REQUESTS");
        assert!(text.says_usage_limit());
        assert!(prompt_result(&refusal(), &text).is_some());
    }

    #[test]
    fn a_plain_refusal_or_a_normal_answer_stays() {
        let mut text = PromptText::default();
        text.push("I'm not able to help with that request.");
        assert_eq!(prompt_result(&refusal(), &text).map(|e| e.to_string()), None);
        let mut quota = PromptText::default();
        quota.push("Usage Limit Reached");
        let done = json!({"stopReason": "end_turn"});
        assert!(prompt_result(&done, &quota).is_none(), "a normal answer is not failed");
    }
}
