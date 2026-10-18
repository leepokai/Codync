use super::send::Failure;
use serde_json::Value;

pub(super) const NOT_RUNNING: &str =
    "codync-host isn't running here. Start it with `codync-host install` (or `serve`).";

pub(super) struct Api {
    pub port: u16,
    token: String,
}

impl Api {
    pub fn new(port: u16) -> Result<Self, Failure> {
        let token = std::fs::read_to_string(crate::service::data_dir().join("token"))
            .map_err(|_| Failure::api(NOT_RUNNING))?
            .trim()
            .to_owned();
        if token.is_empty() {
            return Err(Failure::api(NOT_RUNNING));
        }
        Ok(Self { port, token })
    }

    pub fn redact(&self, text: &str) -> String {
        text.replace(&self.token, "[redacted]")
    }

    pub async fn call(&self, method: &str, body: Value) -> Result<Value, Failure> {
        crate::local_call(self.port, method, body).await.map_err(|e| Failure::api(self.redact(&e.to_string())))
    }

    pub async fn send(&self, body: Value) -> Result<Value, Failure> {
        // A lost response may follow a committed send. Every attempt uses the same nonce.
        for attempt in 0..3 {
            match self.call("send", body.clone()).await {
                Err(e) if matches!(e.message.as_str(), NOT_RUNNING | "reading the host's answer") && attempt < 2 => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                result => return result,
            }
        }
        unreachable!("the last attempt returns")
    }

    pub async fn events(&self) -> Result<reqwest::Response, Failure> {
        let request = crate::http()
            .get(format!("http://127.0.0.1:{}/events?since=0&client=cli-send", self.port))
            .bearer_auth(&self.token)
            .send();
        let response = tokio::time::timeout(std::time::Duration::from_secs(20), request)
            .await
            .map_err(|_| Failure::api(NOT_RUNNING))?
            .map_err(|_| Failure::api(NOT_RUNNING))?;
        if !response.status().is_success() {
            let body: Value = response.json().await.map_err(|_| Failure::api("the host refused"))?;
            return Err(Failure::api(self.redact(body["error"].as_str().unwrap_or("the host refused"))));
        }
        Ok(response)
    }
}
