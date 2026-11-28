//! Native HTTP client and ordered failover orchestration for decision sidecar calls.

use super::breaker::{Admission, Breaker, Ticket};
use super::config::SidecarTarget;
use super::{DecisionCallOutcome, DecisionCallReport};
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

struct ProbeGuard<'a> {
    breaker: &'a Breaker,
    key: &'a str,
    ticket: Ticket,
    armed: bool,
}

impl<'a> ProbeGuard<'a> {
    fn new(breaker: &'a Breaker, key: &'a str, ticket: Ticket) -> Self {
        Self { breaker, key, ticket, armed: ticket.is_probe }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.breaker.abort_probe(self.key, self.ticket);
        }
    }
}

enum TargetOutcome {
    Hit(String),
    Miss(DecisionCallOutcome),
}

pub(crate) async fn decision_call(targets: &[SidecarTarget], system: &str, prompt: &str) -> DecisionCallReport {
    if targets.is_empty() {
        return DecisionCallReport {
            content: None,
            targets: 0,
            attempted: 0,
            outcome: DecisionCallOutcome::Unconfigured,
            ms: 0,
        };
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
) -> DecisionCallReport {
    let started = Instant::now();
    if targets.is_empty() {
        return DecisionCallReport {
            content: None,
            targets: 0,
            attempted: 0,
            outcome: DecisionCallOutcome::Unconfigured,
            ms: 0,
        };
    }

    let mut attempted = 0usize;
    let mut last_reason = DecisionCallOutcome::CoolingDown;
    for (index, target) in targets.iter().enumerate() {
        let remaining = total_budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            tracing::warn!(index, targets = targets.len(), "decision sidecar total budget exhausted");
            last_reason = DecisionCallOutcome::BudgetExhausted;
            break;
        }
        let bkey = target.breaker_key();
        let label = target.redacted_label();
        let ticket = match breaker.try_acquire(&bkey) {
            Admission::Admit(ticket) => ticket,
            Admission::Skip { reason } => {
                tracing::debug!(target = %label, index, reason = reason.as_str(), "skipping cooling decision sidecar target");
                last_reason = reason;
                continue;
            }
        };
        attempted += 1;
        let req_timeout = per_request_timeout.min(remaining);
        match try_target(client, breaker, target, &bkey, &label, index, ticket, system, prompt, req_timeout).await {
            TargetOutcome::Hit(content) => {
                return DecisionCallReport {
                    content: Some(content),
                    targets: targets.len(),
                    attempted,
                    outcome: DecisionCallOutcome::Hit,
                    ms: elapsed_ms(started),
                };
            }
            TargetOutcome::Miss(reason) => last_reason = reason,
        }
    }

    tracing::warn!(targets = targets.len(), "all decision sidecar targets failed or cooling; failing open");
    DecisionCallReport {
        content: None,
        targets: targets.len(),
        attempted,
        outcome: last_reason,
        ms: elapsed_ms(started),
    }
}

#[allow(clippy::too_many_arguments)]
async fn try_target(
    client: &reqwest::Client,
    breaker: &Breaker,
    target: &SidecarTarget,
    bkey: &str,
    label: &str,
    index: usize,
    ticket: Ticket,
    system: &str,
    prompt: &str,
    req_timeout: Duration,
) -> TargetOutcome {
    let mut guard = ProbeGuard::new(breaker, bkey, ticket);
    let started = Instant::now();
    let response = match send_chat_completion(client, target, system, prompt, req_timeout).await {
        Ok(resp) => resp,
        Err(reason) => {
            let cooled = breaker.record_transient_failure(bkey, ticket, reason);
            guard.disarm();
            tracing::warn!(target = %label, index, reason = reason.as_str(), cooled, "decision sidecar request error");
            return TargetOutcome::Miss(reason);
        }
    };
    let body_budget = req_timeout.saturating_sub(started.elapsed());
    let outcome = handle_response(breaker, bkey, label, index, ticket, response, body_budget).await;
    guard.disarm();
    outcome
}

async fn send_chat_completion(
    client: &reqwest::Client,
    target: &SidecarTarget,
    system: &str,
    prompt: &str,
    req_timeout: Duration,
) -> Result<reqwest::Response, DecisionCallOutcome> {
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
    match tokio::time::timeout(req_timeout, req.send()).await {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(error)) => {
            Err(if error.is_timeout() { DecisionCallOutcome::Timeout } else { DecisionCallOutcome::Transport })
        }
        Err(_) => Err(DecisionCallOutcome::Timeout),
    }
}

async fn handle_response(
    breaker: &Breaker,
    bkey: &str,
    label: &str,
    index: usize,
    ticket: Ticket,
    response: reqwest::Response,
    body_budget: Duration,
) -> TargetOutcome {
    let status = response.status();
    if status.is_success() {
        return handle_success_body(breaker, bkey, label, index, ticket, response, body_budget).await;
    }
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        breaker.record_payment_required(bkey, ticket);
        tracing::warn!(target = %label, index, status = 402, "decision sidecar returned 402; entering 30m cooldown");
        return TargetOutcome::Miss(DecisionCallOutcome::Http402);
    }
    let reason = classify_non_success_status(status);
    let cooled = breaker.record_transient_failure(bkey, ticket, reason);
    tracing::warn!(
        target = %label,
        index,
        status = status.as_u16(),
        reason = reason.as_str(),
        cooled,
        "decision sidecar returned non-success status"
    );
    TargetOutcome::Miss(reason)
}

async fn handle_success_body(
    breaker: &Breaker,
    bkey: &str,
    label: &str,
    index: usize,
    ticket: Ticket,
    response: reqwest::Response,
    body_budget: Duration,
) -> TargetOutcome {
    let parsed = tokio::time::timeout(body_budget, response.json::<Value>()).await;
    match parsed {
        Ok(Ok(json)) => {
            if let Some(content) = extract_content(&json) {
                breaker.record_success(bkey, ticket);
                return TargetOutcome::Hit(content);
            }
            let cooled = breaker.record_transient_failure(bkey, ticket, DecisionCallOutcome::EmptyResponse);
            tracing::warn!(target = %label, index, cooled, "decision sidecar returned empty choices");
            TargetOutcome::Miss(DecisionCallOutcome::EmptyResponse)
        }
        Ok(Err(_)) => {
            let cooled = breaker.record_transient_failure(bkey, ticket, DecisionCallOutcome::InvalidJson);
            tracing::warn!(target = %label, index, cooled, "decision sidecar returned invalid JSON");
            TargetOutcome::Miss(DecisionCallOutcome::InvalidJson)
        }
        Err(_) => {
            let cooled = breaker.record_transient_failure(bkey, ticket, DecisionCallOutcome::Timeout);
            tracing::warn!(target = %label, index, cooled, "decision sidecar body timed out");
            TargetOutcome::Miss(DecisionCallOutcome::Timeout)
        }
    }
}

fn classify_non_success_status(status: reqwest::StatusCode) -> DecisionCallOutcome {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        DecisionCallOutcome::Http429
    } else if status.is_server_error() {
        DecisionCallOutcome::Http5xx
    } else {
        DecisionCallOutcome::Http4xx
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
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
        assert_eq!(out.content, None);
        assert_eq!(out.outcome, DecisionCallOutcome::Unconfigured);
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
        assert_eq!(out.content.as_deref(), Some("name: Release Captain"));
        assert_eq!(out.outcome, DecisionCallOutcome::Hit);
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
        assert_eq!(first.content.as_deref(), Some("name: 記帳助手"));
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
        assert_eq!(second.content.as_deref(), Some("name: 記帳助手"));
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
                .await
                .content,
                None
            );
        }
        assert_eq!(bad.hits.load(Ordering::SeqCst), 3, "4th call must skip cooled 500 target");

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
                .await
                .content,
                None
            );
        }
        assert_eq!(rate.hits.load(Ordering::SeqCst), 3, "4th call must skip cooled 429 target");

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
                .await
                .content,
                None
            );
        }
        assert_eq!(slow.hits.load(Ordering::SeqCst), 3, "4th call must skip cooled timed-out target");
    }

    #[tokio::test]
    async fn all_targets_unavailable_fails_open_with_none() {
        let (u1, m1) = spawn_mock(StatusCode::BAD_GATEWAY, json!({"error": "bad gateway"}), Duration::ZERO).await;
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
        assert_eq!(out.content, None);
        assert_eq!(m1.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn expired_cooldown_permits_only_one_concurrent_probe() {
        let (url, mock) = spawn_mock(StatusCode::OK, ok_body("name: Recovered"), Duration::from_millis(120)).await;
        let target = SidecarTarget { url, model: "m".into(), api_key: None };
        let bkey = target.breaker_key();
        let breaker = Arc::new(Breaker::default());
        let past = Instant::now().checked_sub(super::super::breaker::COOLDOWN_PAYMENT_REQUIRED).unwrap();
        let Admission::Admit(t) = breaker.try_acquire_at(&bkey, past) else { unreachable!() };
        breaker.record_payment_required_at(&bkey, past, t);

        let client = reqwest::Client::new();
        let targets = vec![target];
        let (r1, r2) = tokio::join!(
            decision_call_with(
                &client,
                &breaker,
                &targets,
                "sys",
                "usr",
                Duration::from_secs(1),
                Duration::from_secs(2)
            ),
            decision_call_with(
                &client,
                &breaker,
                &targets,
                "sys",
                "usr",
                Duration::from_secs(1),
                Duration::from_secs(2)
            ),
        );
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1, "only one caller may probe half-open target");
        let hits = [r1.content.is_some(), r2.content.is_some()].into_iter().filter(|ok| *ok).count();
        assert_eq!(hits, 1);
    }
}
