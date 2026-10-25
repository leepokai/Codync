//! `CodeBuddy`'s ACP extensions (its CLI runs `CodeBuddy` Code and `WorkBuddy`): a prompt that fails answers `refusal`
//! with the error in `_meta["codebuddy.ai/errorMessage"]`, `authenticate` posts the sign-in page on
//! `_codebuddy.ai/authUrl`, and `_codebuddy.ai/getUserInfo` names the signed-in account.

use super::{Acp, Incoming, rpc_error};
use anyhow::Result;
use serde_json::{Value, json};
use std::time::Duration;

/// A `session/prompt` answer, with the error `CodeBuddy` hides in a refusal returned as that error, so a sign-in, a
/// fallback or the agent's own words follow from it as from any other agent's error answer.
pub fn prompt_result(result: Value) -> Result<Value> {
    let Some(raw) = result["_meta"]["codebuddy.ai/errorMessage"].as_str().filter(|_| result["stopReason"] == "refusal")
    else {
        return Ok(result);
    };
    let error =
        serde_json::from_str::<Value>(raw).ok().filter(Value::is_object).unwrap_or_else(|| json!({"message": raw}));
    Err(rpc_error(&error).into())
}

/// The sign-in page `authenticate` posted, when it is a web page: the host opens it in the browser.
pub fn sign_in_url(incoming: &Incoming) -> Option<String> {
    let Incoming::Notification { method, params } = incoming else { return None };
    let url = params["authUrl"].as_str().filter(|_| method == "_codebuddy.ai/authUrl")?;
    let url = reqwest::Url::parse(url).ok()?;
    (matches!(url.scheme(), "https" | "http") && url.has_host()).then(|| url.into())
}

/// Whether an account is signed in (signed out, the answer has no `userInfo`). `None`: it didn't say. Nothing else
/// of the answer is read: it carries the account's tokens.
pub async fn signed_in(acp: &Acp) -> Option<bool> {
    let info = tokio::time::timeout(Duration::from_secs(15), acp.request("_codebuddy.ai/getUserInfo", json!({})));
    info.await.ok()?.ok().map(|info| !info["userInfo"].is_null())
}

#[cfg(test)]
mod tests {
    use super::super::{AUTH_REQUIRED, RpcError, needs_sign_in};
    use super::*;

    fn refusal(error: &str) -> Value {
        json!({"stopReason": "refusal", "_meta": {"codebuddy.ai/errorMessage": error}})
    }

    fn error_of(result: Value) -> anyhow::Error {
        prompt_result(result).expect_err("an error in the refusal")
    }

    #[test]
    fn a_refusal_that_hides_a_sign_in_error_asks_to_sign_in() {
        let error = error_of(refusal(
            r#"{"code":-32000,"message":"Authentication required","data":{"details":"Please use /login command to sign in to your account","category":"auth"}}"#,
        ));
        let rpc = error.downcast_ref::<RpcError>().expect("an RpcError");
        assert_eq!((rpc.code, rpc.kind.as_deref()), (AUTH_REQUIRED, Some("auth")));
        assert!(needs_sign_in(&error));
        assert!(error.to_string().contains("sign in to your account"));
    }

    #[test]
    fn a_custom_model_key_error_shows_its_own_words() {
        let error = error_of(refusal(
            r#"{"code":-32603,"message":"Internal error","data":{"details":"Authentication failed for custom model my-model. Check if the apiKey in your models.json is correct","category":"custom_model_auth"}}"#,
        ));
        assert!(!needs_sign_in(&error), "signing in doesn't fix a bad key in models.json");
        assert!(error.to_string().contains("models.json"));
    }

    #[test]
    fn a_plain_refusal_or_answer_stays() {
        let plain = json!({"stopReason": "refusal"});
        assert_eq!(prompt_result(plain.clone()).unwrap(), plain);
        let done = json!({"stopReason": "end_turn", "_meta": {"codebuddy.ai/errorMessage": "{}"}});
        assert_eq!(prompt_result(done.clone()).unwrap(), done);
        assert_eq!(error_of(refusal("Network unreachable")).to_string(), "Network unreachable");
    }

    #[test]
    fn only_a_web_page_is_opened() {
        let posted = |url: &str| Incoming::Notification {
            method: "_codebuddy.ai/authUrl".into(),
            params: json!({"authUrl": url}),
        };
        assert_eq!(
            sign_in_url(&posted("https://www.workbuddy.ai/login?state=a b")).as_deref(),
            Some("https://www.workbuddy.ai/login?state=a%20b")
        );
        for refused in ["file:///etc/passwd", "javascript:alert(1)", "workbuddy://login", "https://", "not a url"] {
            assert_eq!(sign_in_url(&posted(refused)), None, "{refused}");
        }
        let other =
            Incoming::Notification { method: "session/update".into(), params: json!({"authUrl": "https://x.dev"}) };
        assert_eq!(sign_in_url(&other), None);
    }
}
