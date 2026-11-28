//! Native HTTP client and ordered failover orchestration for decision sidecar calls.

use super::breaker::Breaker;
use super::config::SidecarTarget;
use serde_json::{Value, json};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub(crate) const PER_REQUEST_TIMEOUT: Duration = Duration::from_millis(2500);
pub(crate) const TOTAL_BUDGET: Duration = Duration::from_millis(5000);

pub(crate) fn shared_client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(4)
            .connect_timeout(Duration::from_millis(1500))
            .timeout(PER_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    });
    &CLIENT
}

pub(crate) fn global_breaker() -> &'static Breaker {
    static BREAKER: LazyLock<Breaker> = LazyLock::new(Breaker::default);
    &BREAKER
}

pub(crate) async fn decision_call(targets: &[SidecarTarget], system: &str, prompt: &str) -> Option<String> {
    if targets.is_empty() {
        return None;
    }
    decision_call_with(shared_client(), global_breaker(), targets, system, prompt, PER_REQUEST_TIMEOUT, TOTAL_BUDGET)
        .await
}

pub(crate) async fn decision_call_with(
    client: &reqwest::Client,
    breaker: &Breaker,
    targets: &[SidecarTarget],
    system: &str,
    prompt: &str,
    per_request_timeout: Duration,
    total_budget: Duration,
) -> Option<String> {
    if targets.is_empty() {
        return None;
    }

    let started = Instant::now();
    for (index, target) in targets.iter().enumerate() {
        let remaining = total_budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            tracing::warn!(index, targets = targets.len(), "decision sidecar total budget exhausted");
            break;
        }

        let bkey = target.breaker_key();
        let label = target.redacted_label();
        if let Some(reason) = breaker.is_cooling_down(&bkey) {
            tracing::debug!(target = %label, index, reason, "skipping cooling decision sidecar target");
            continue;
        }

        let req_timeout = per_request_timeout.min(remaining);
        let endpoint = target.endpoint_url();
        let payload = json!({
            "model": target.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.1,
            "max_tokens": 128
        });

        let mut req = client.post(&endpoint).timeout(req_timeout).json(&payload);
        if let Some(key) = target.api_key.as_deref().filter(|k| !k.is_empty()) {
            req = req.bearer_auth(key);
        }

        let response = match tokio::time::timeout(req_timeout, req.send()).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(error)) => {
                let kind = if error.is_timeout() { "timeout" } else { "transport" };
                let cooled = breaker.record_transient_failure(&bkey, kind);
                tracing::warn!(target = %label, index, reason = kind, cooled, "decision sidecar request error");
                continue;
            }
            Err(_) => {
                let cooled = breaker.record_transient_failure(&bkey, "timeout");
                tracing::warn!(target = %label, index, reason = "timeout", cooled, "decision sidecar request timed out");
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            let body_left = total_budget.saturating_sub(started.elapsed()).min(req_timeout);
            let parsed = tokio::time::timeout(body_left, response.json::<Value>()).await;
            match parsed {
                Ok(Ok(json)) => {
                    if let Some(content) = extract_content(&json) {
                        breaker.record_success(&bkey);
                        return Some(content);
                    }
                    let cooled = breaker.record_transient_failure(&bkey, "empty_response");
                    tracing::warn!(target = %label, index, cooled, "decision sidecar returned empty choices");
                }
                Ok(Err(_)) => {
                    let cooled = breaker.record_transient_failure(&bkey, "invalid_json");
                    tracing::warn!(target = %label, index, cooled, "decision sidecar returned invalid JSON");
                }
                Err(_) => {
                    let cooled = breaker.record_transient_failure(&bkey, "timeout");
                    tracing::warn!(target = %label, index, cooled, "decision sidecar body timed out");
                }
            }
            continue;
        }

        if status == reqwest::StatusCode::PAYMENT_REQUIRED {
            breaker.record_payment_required(&bkey);
            tracing::warn!(
                target = %label,
                index,
                status = 402,
                "decision sidecar target returned 402; entering 30m cooldown"
            );
            continue;
        }

        let reason = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            "http_429"
        } else if status.is_server_error() {
            "http_5xx"
        } else {
            "http_4xx"
        };
        let cooled = breaker.record_transient_failure(&bkey, reason);
        tracing::warn!(
            target = %label,
            index,
            status = status.as_u16(),
            reason,
            cooled,
            "decision sidecar target returned non-success status"
        );
    }

    tracing::warn!(targets = targets.len(), "all decision sidecar targets failed or cooling; failing open");
    None
}

fn extract_content(json: &Value) -> Option<String> {
    let message = json.get("choices")?.get(0)?.get("message")?;
    let content = message.get("content")?;
    if let Some(text) = content.as_str() {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_owned());
    }
    if let Some(parts) = content.as_array() {
        let joined: String = parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect();
        let trimmed = joined.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct MockState {
        hits: Arc<AtomicUsize>,
        seen_auth: Arc<std::sync::Mutex<Vec<Option<String>>>>,
        status: StatusCode,
        body: Value,
        delay: Duration,
    }

    async fn mock_handler(
        State(state): State<MockState>,
        headers: HeaderMap,
        Json(_req): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.hits.fetch_add(1, Ordering::SeqCst);
        let auth = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(str::to_owned);
        state.seen_auth.lock().unwrap().push(auth);
        if !state.delay.is_zero() {
            tokio::time::sleep(state.delay).await;
        }
        (state.status, Json(state.body))
    }

    async fn spawn_mock(status: StatusCode, body: Value, delay: Duration) -> (String, MockState) {
        let state = MockState {
            hits: Arc::new(AtomicUsize::new(0)),
            seen_auth: Arc::new(std::sync::Mutex::new(Vec::new())),
            status,
            body,
            delay,
        };
        let app = Router::new().route("/v1/chat/completions", post(mock_handler)).with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}/v1"), state)
    }

    fn ok_body(text: &str) -> Value {
        json!({"choices": [{"message": {"role": "assistant", "content": text}}]})
    }

    #[tokio::test]
    async fn unconfigured_returns_none_with_zero_requests() {
        let client = reqwest::Client::new();
        let breaker = Breaker::default();
        let out =
            decision_call_with(&client, &breaker, &[], "sys", "usr", Duration::from_secs(1), Duration::from_secs(2))
                .await;
        assert_eq!(out, None);
    }

    #[tokio::test]
    async fn successful_target_returns_content_and_sends_bearer_token() {
        let (url, mock) = spawn_mock(StatusCode::OK, ok_body("name: Release Captain"), Duration::ZERO).await;
        let targets = vec![SidecarTarget { url, model: "fast-model".into(), api_key: Some("tok-123".into()) }];
        let client = reqwest::Client::new();
        let breaker = Breaker::default();
        let out = decision_call_with(
            &client,
            &breaker,
            &targets,
            "sys",
            "usr",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(out.as_deref(), Some("name: Release Captain"));
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1);
        assert_eq!(mock.seen_auth.lock().unwrap().as_slice(), &[Some("Bearer tok-123".to_owned())]);
    }

    #[tokio::test]
    async fn failover_on_402_switches_to_backup_and_skips_cooled_primary_next_call() {
        let (primary_url, primary) =
            spawn_mock(StatusCode::PAYMENT_REQUIRED, json!({"error": "insufficient_quota"}), Duration::ZERO).await;
        let (backup_url, backup) = spawn_mock(StatusCode::OK, ok_body("name: 記帳助手"), Duration::ZERO).await;
        let targets = vec![
            SidecarTarget { url: primary_url, model: "m1".into(), api_key: Some("k1".into()) },
            SidecarTarget { url: backup_url, model: "m2".into(), api_key: None },
        ];
        let client = reqwest::Client::new();
        let breaker = Breaker::default();

        // Call 1: primary hits 402 -> immediately fails over to backup.
        let first = decision_call_with(
            &client,
            &breaker,
            &targets,
            "sys",
            "usr",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(first.as_deref(), Some("name: 記帳助手"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(backup.hits.load(Ordering::SeqCst), 1);

        // Call 2: primary is in 30m cooldown -> skipped with ZERO HTTP calls, goes straight to backup.
        let second = decision_call_with(
            &client,
            &breaker,
            &targets,
            "sys",
            "usr",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(second.as_deref(), Some("name: 記帳助手"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1, "402 target must not be hit again during cooldown");
        assert_eq!(backup.hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn three_consecutive_500_or_429_or_timeouts_open_breaker_and_skip_subsequent_calls() {
        let (bad_url, bad) =
            spawn_mock(StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "boom"}), Duration::ZERO).await;
        let (rate_url, rate) =
            spawn_mock(StatusCode::TOO_MANY_REQUESTS, json!({"error": "slow down"}), Duration::ZERO).await;
        let (slow_url, slow) = spawn_mock(StatusCode::OK, ok_body("name: Too Late"), Duration::from_millis(300)).await;
        let client = reqwest::Client::new();

        // 5xx x3 -> cools down on 3rd failure, 4th call makes zero requests.
        let breaker_500 = Breaker::default();
        let t_500 = vec![SidecarTarget { url: bad_url, model: "m".into(), api_key: None }];
        for _ in 0..4 {
            assert_eq!(
                decision_call_with(
                    &client,
                    &breaker_500,
                    &t_500,
                    "sys",
                    "usr",
                    Duration::from_secs(1),
                    Duration::from_secs(2)
                )
                .await,
                None
            );
        }
        assert_eq!(bad.hits.load(Ordering::SeqCst), 3, "4th call must skip cooled 500 target");

        // 429 x3 -> cools down on 3rd failure, 4th call makes zero requests.
        let breaker_429 = Breaker::default();
        let t_429 = vec![SidecarTarget { url: rate_url, model: "m".into(), api_key: None }];
        for _ in 0..4 {
            assert_eq!(
                decision_call_with(
                    &client,
                    &breaker_429,
                    &t_429,
                    "sys",
                    "usr",
                    Duration::from_secs(1),
                    Duration::from_secs(2)
                )
                .await,
                None
            );
        }
        assert_eq!(rate.hits.load(Ordering::SeqCst), 3, "4th call must skip cooled 429 target");

        // Timeout x3 -> cools down on 3rd failure, 4th call makes zero requests.
        let breaker_to = Breaker::default();
        let t_to = vec![SidecarTarget { url: slow_url, model: "m".into(), api_key: None }];
        for _ in 0..4 {
            assert_eq!(
                decision_call_with(
                    &client,
                    &breaker_to,
                    &t_to,
                    "sys",
                    "usr",
                    Duration::from_millis(50),
                    Duration::from_millis(200)
                )
                .await,
                None
            );
        }
        assert_eq!(slow.hits.load(Ordering::SeqCst), 3, "4th call must skip cooled timed-out target");
    }

    #[tokio::test]
    async fn all_targets_unavailable_fails_open_with_none() {
        let (u1, m1) = spawn_mock(StatusCode::BAD_GATEWAY, json!({"error": "bad gateway"}), Duration::ZERO).await;
        // Closed port -> immediate connection refused transport error.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_addr = listener.local_addr().unwrap();
        drop(listener);
        let targets = vec![
            SidecarTarget { url: u1, model: "m1".into(), api_key: None },
            SidecarTarget { url: format!("http://{closed_addr}/v1"), model: "m2".into(), api_key: None },
        ];
        let client = reqwest::Client::new();
        let breaker = Breaker::default();
        let out = decision_call_with(
            &client,
            &breaker,
            &targets,
            "sys",
            "usr",
            Duration::from_secs(1),
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(out, None);
        assert_eq!(m1.hits.load(Ordering::SeqCst), 1);
    }
}
