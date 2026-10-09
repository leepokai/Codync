//! Private encrypted channel subscriptions.

use super::*;
use tokio::sync::watch;

fn screen_authorized(hub: &Hub, owner: &str) -> bool {
    devices::authorize(hub, owner).is_ok_and(|device| device.scopes.contains(&Scope::Screen))
}

/// Keep checking permissions even while a slow device fills the outbound queue.
pub(super) async fn send_screen_event(
    hub: &Hub,
    owner: &str,
    auth: &mut watch::Receiver<u64>,
    tx: &mpsc::Sender<ToDevice>,
    packet: Value,
) -> bool {
    if !screen_authorized(hub, owner) {
        return false;
    }
    let send = tx.send(ToDevice::Inner(packet));
    tokio::pin!(send);
    loop {
        tokio::select! {
            biased;
            changed = auth.changed() => {
                if changed.is_err() || !screen_authorized(hub, owner) { return false; }
            }
            result = &mut send => return result.is_ok(),
        }
    }
}

impl Channel {
    pub(super) async fn subscribe(&self, id: u64, kind: &str, b: &Value) {
        if self.count(true) >= MAX_SUBS {
            self.send(err(id, 429, "too many subscriptions")).await;
            return;
        }
        // Subscribe before reserving the receiver so permission changes cannot fall
        // between the initial authorization and the spawned subscription task.
        let mut auth = self.hub.auth.subscribe();
        let stream = match kind {
            "events" => match api::events_stream(
                &self.hub,
                b["since"].as_i64().unwrap_or(0),
                b["client"].as_str(),
                &self.caller,
            ) {
                Ok(s) => s.boxed(),
                Err(e) => {
                    self.send(err(id, 500, &format!("{e:#}"))).await;
                    return;
                }
            },
            "screenCandidates" => {
                if let Err(error) = devices::permit(&self.caller, "screenCandidates") {
                    self.send(err(id, 403, &error.to_string())).await;
                    return;
                }
                match self.hub.screen.candidates(b["session"].as_str().unwrap_or_default(), self.caller.device_key()) {
                    Ok(stream) => stream.boxed(),
                    Err(error) => {
                        self.send(err(id, 400, &error.to_string())).await;
                        return;
                    }
                }
            }
            "term" => {
                let Some(s) = api::term_events(&self.hub, b["term"].as_str().unwrap_or_default()) else {
                    self.send(err(id, 404, "that terminal is gone")).await;
                    return;
                };
                s.boxed()
            }
            _ => {
                self.send(err(id, 400, "unknown subscription")).await;
                return;
            }
        };
        let screen_subscription = kind == "screenCandidates";
        let hub = self.hub.clone();
        let owner = self.caller.device_key().to_owned();
        let (jobs, tx) = (self.jobs.clone(), self.tx.clone());
        // Held while spawning so the task can't finish (and remove itself) before it's listed.
        let mut list = self.jobs.locked();
        let task = tokio::spawn(async move {
            let mut stream = stream;
            loop {
                if screen_subscription && !screen_authorized(&hub, &owner) {
                    break;
                }
                let ev = tokio::select! {
                    biased;
                    changed = auth.changed(), if screen_subscription => {
                        if changed.is_err() || !screen_authorized(&hub, &owner) { break; }
                        continue;
                    }
                    event = stream.next() => { let Some(event) = event else { break }; event }
                };
                let packet = json!({"id": id, "ev": ev});
                let sent = if screen_subscription {
                    send_screen_event(&hub, &owner, &mut auth, &tx, packet).await
                } else {
                    let result = tx.send(ToDevice::Inner(packet)).await;
                    result.is_ok()
                };
                if !sent {
                    break;
                }
            }
            // Dropping a screen receiver closes its viewer, even if the final end
            // notification cannot yet fit in this device's outgoing queue.
            drop(stream);
            if jobs.locked().remove(&id).is_some() {
                let _ = tx.send(ToDevice::Inner(json!({"id": id, "end": true}))).await;
            }
        });
        list.insert(id, Job::Sub(task.abort_handle()));
    }
}
