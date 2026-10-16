use super::*;
use crate::screen::helper::Link;
use crate::screen::{IceConfig, protocol::HelperStatus};
use std::sync::{Mutex, atomic::AtomicI64};
use tokio::sync::broadcast;

fn fixture(trickle: bool) -> (Arc<Screen>, mpsc::UnboundedReceiver<String>) {
    let (events, _) = broadcast::channel(8);
    let screen = Arc::new(Screen::new(Some(true), events));
    let (tx, rx) = mpsc::unbounded_channel();
    *screen.link.locked() = Some(Arc::new(Link { id: 1, tx, pending: Mutex::default(), next_id: AtomicI64::new(1) }));
    *screen.status.locked() = HelperStatus { capture: true, trickle, ..HelperStatus::default() };
    (screen, rx)
}

fn prepared(screen: &Screen, requested: bool) -> String {
    let result = screen.prepare("phone", IceConfig::default(), requested).unwrap();
    result["session"].as_str().unwrap().to_owned()
}

fn candidate(n: usize) -> Value {
    json!({"type": "candidate", "candidate": format!("candidate:{n} 1 UDP 1 192.0.2.1 5000 typ relay"), "sdpMLineIndex": 0})
}

async fn answer(screen: &Arc<Screen>, requests: &mut mpsc::UnboundedReceiver<String>, session: &str) {
    let task = tokio::spawn({
        let screen = screen.clone();
        let session = session.to_owned();
        async move { screen.offer("phone", "candidate-free offer", &session, None).await }
    });
    let request = requests.recv().await;
    let request = request.unwrap();
    let request: Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["params"]["trickle"], true);
    screen.helper_candidate(session, candidate(1));
    let link = screen.link().unwrap();
    link.pending
        .locked()
        .remove(&request["id"].as_i64().unwrap())
        .unwrap()
        .send(Ok(json!({"sdp": "candidate-free answer"})))
        .unwrap();
    let result = task.await;
    let result = result.unwrap().unwrap();
    assert_eq!(result["sdp"], "candidate-free answer");
}

#[tokio::test]
async fn preparation_negotiates_support_once_and_falls_back_for_old_peers() {
    let (screen, _requests) = fixture(false);
    let session = prepared(&screen, true);
    assert!(screen.sessions.locked()[&session].signaling.is_none());
    assert!(screen.candidates(&session, "phone").is_err());
    screen.status.locked().trickle = true;
    let session = prepared(&screen, false);
    assert!(screen.sessions.locked()[&session].signaling.is_none());
    let result = screen.prepare("phone", IceConfig::default(), true).unwrap();
    assert_eq!(result["trickle"], true);
}

#[tokio::test]
async fn private_stream_is_ready_before_offer_and_keeps_late_candidates_in_order() {
    let (screen, mut requests) = fixture(true);
    let session = prepared(&screen, true);
    assert!(screen.candidates(&session, "other").is_err());
    let result = screen.offer("phone", "offer", &session, None).await;
    assert!(result.is_err());
    let mut stream = screen.candidates(&session, "phone").unwrap().boxed();
    let ready = stream.next().await;
    assert_eq!(ready.unwrap()["type"], "ready");
    assert!(screen.candidates(&session, "phone").is_err());
    answer(&screen, &mut requests, &session).await;
    let first = stream.next().await;
    assert_eq!(first.unwrap(), candidate(1));
    screen.helper_candidate(&session, candidate(2));
    screen.helper_candidate(&session, json!({"type": "complete"}));
    let second = stream.next().await;
    assert_eq!(second.unwrap(), candidate(2));
    let completed = stream.next().await;
    assert_eq!(completed.unwrap()["type"], "complete");
    let result = screen.offer("phone", "again", &session, None).await;
    assert!(result.is_err());
    let result = screen.candidate(&session, "other", Candidate::Complete).await;
    assert!(result.is_err());
    screen.sessions.locked().get_mut(&session).unwrap().ice.expires_at = 0;
    let result = screen.candidate(&session, "phone", Candidate::Complete).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn concurrent_offers_and_pre_answer_uploads_are_rejected() {
    let (screen, mut requests) = fixture(true);
    let session = prepared(&screen, true);
    let _stream = screen.candidates(&session, "phone").unwrap().boxed();
    let offering = tokio::spawn({
        let screen = screen.clone();
        let session = session.clone();
        async move { screen.offer("phone", "offer", &session, None).await }
    });
    let request = requests.recv().await;
    let request = request.unwrap();
    let request: Value = serde_json::from_str(&request).unwrap();
    let result = screen.offer("phone", "offer", &session, None).await;
    assert!(result.is_err());
    let result = screen.candidate(&session, "phone", Candidate::Complete).await;
    assert!(result.is_err());
    let link = screen.link().unwrap();
    link.pending.locked().remove(&request["id"].as_i64().unwrap()).unwrap().send(Ok(json!({"sdp": "answer"}))).unwrap();
    let result = offering.await;
    result.unwrap().unwrap();
}

#[tokio::test]
async fn overflow_and_post_completion_candidates_fail_without_delivering_stale_data() {
    for post_completion in [false, true] {
        let (screen, mut requests) = fixture(true);
        let session = prepared(&screen, true);
        let mut stream = screen.candidates(&session, "phone").unwrap().boxed();
        let ready = stream.next().await;
        ready.unwrap();
        answer(&screen, &mut requests, &session).await;
        if post_completion {
            screen.helper_candidate(&session, json!({"type": "complete"}));
        } else {
            for n in 2..=128 {
                screen.helper_candidate(&session, candidate(n));
            }
        }
        screen.helper_candidate(&session, candidate(129));
        let error = stream.next().await;
        assert_eq!(error.unwrap()["type"], "error");
        let ended = stream.next().await;
        assert!(ended.is_none());
    }
}

#[tokio::test]
async fn signalling_loss_prevents_resubscription_and_a_fresh_session_can_connect() {
    let (screen, _requests) = fixture(true);
    let session = prepared(&screen, true);
    let stream = screen.candidates(&session, "phone").unwrap().boxed();
    drop(stream);
    assert!(screen.candidates(&session, "phone").is_err());
    let result = screen.offer("phone", "offer", &session, None).await;
    assert!(result.is_err());
    let fresh = prepared(&screen, true);
    assert_ne!(fresh, session);
    assert!(screen.candidates(&fresh, "phone").is_ok());
}

#[tokio::test]
async fn connected_media_survives_candidate_subscription_loss() {
    let (screen, mut requests) = fixture(true);
    let session = prepared(&screen, true);
    let mut stream = screen.candidates(&session, "phone").unwrap().boxed();
    let ready = stream.next().await;
    ready.unwrap();
    answer(&screen, &mut requests, &session).await;
    screen.session_connected(&session);
    screen.takeover(true);
    drop(stream);
    // Candidates already in flight after the channel disconnects must not fail healthy media.
    screen.helper_candidate(&session, candidate(2));
    let stale_upload = screen.candidate(&session, "phone", Candidate::Complete).await;
    assert!(stale_upload.is_err(), "detached candidates never reuse the old signaling exchange");
    assert!(screen.owns_session(&session, "phone"));
    assert_eq!(screen.state()["viewers"], 1);
    assert_eq!(screen.state()["userControl"], true);
    assert!(!screen.sessions.locked()[&session].expired(crate::store::now_ms()));
    tokio::task::yield_now().await;
    assert!(requests.try_recv().is_err(), "signaling loss never sends a media close");
}

#[tokio::test]
async fn completed_exchange_survives_subscription_loss_only_after_delivery_and_acknowledgment() {
    for (delivered, acknowledged) in [(false, true), (true, false), (true, true)] {
        let (screen, mut requests) = fixture(true);
        let session = prepared(&screen, true);
        let mut stream = Some(screen.candidates(&session, "phone").unwrap().boxed());
        let ready = stream.as_mut().unwrap().next().await;
        ready.unwrap();
        answer(&screen, &mut requests, &session).await;
        screen.helper_candidate(&session, json!({"type": "complete"}));
        if delivered {
            let candidate = stream.as_mut().unwrap().next().await;
            assert_eq!(candidate.unwrap()["type"], "candidate");
            let complete = stream.as_mut().unwrap().next().await;
            assert_eq!(complete.unwrap()["type"], "complete");
        }
        let uploading = tokio::spawn({
            let screen = screen.clone();
            let session = session.clone();
            async move { screen.candidate(&session, "phone", Candidate::Complete).await }
        });
        let request = requests.recv().await;
        let request: Value = serde_json::from_str(&request.unwrap()).unwrap();
        let link = screen.link().unwrap();
        let ack = link.pending.locked().remove(&request["id"].as_i64().unwrap()).unwrap();
        let result = if acknowledged {
            ack.send(Ok(json!({}))).unwrap();
            let result = uploading.await;
            Some(result.unwrap())
        } else {
            drop(stream.take());
            ack.send(Ok(json!({}))).unwrap();
            let result = uploading.await;
            Some(result.unwrap())
        };
        if acknowledged {
            result.unwrap().unwrap();
            drop(stream.take());
        } else {
            assert!(result.unwrap().is_err(), "a late completion cannot revive a failed exchange");
        }
        let sessions = screen.sessions.locked();
        let viewer = &sessions[&session];
        assert_eq!(viewer.expired(crate::store::now_ms()), !(delivered && acknowledged));
        assert!(viewer.ice.expires_at > crate::store::now_ms(), "failure is separate from credential expiry");
    }
}

#[tokio::test]
async fn late_connected_notifications_cannot_revive_failed_or_closed_sessions() {
    let (screen, _requests) = fixture(true);
    let session = prepared(&screen, true);
    let stream = screen.candidates(&session, "phone").unwrap().boxed();
    drop(stream);
    screen.session_connected(&session);
    assert!(screen.sessions.locked()[&session].expired(crate::store::now_ms()));
    screen.session_closed(&session);
    screen.session_connected(&session);
    assert!(!screen.owns_session(&session, "phone"));
}

#[test]
fn candidate_bounds_and_completion_apply_to_both_directions() {
    let mut progress = Progress::default();
    for n in 0..128 {
        progress.accept(&serde_json::from_value(candidate(n)).unwrap()).unwrap();
    }
    assert!(progress.accept(&serde_json::from_value(candidate(128)).unwrap()).is_err());
    progress.accept(&Candidate::Complete).unwrap();
    assert!(progress.accept(&Candidate::Complete).is_err());
    let invalid = Candidate::Candidate { candidate: "candidate:bad\nline".into(), sdp_m_line_index: 0, sdp_mid: None };
    assert!(invalid.validate().is_err());
}

#[tokio::test]
async fn candidate_upload_waits_for_acknowledgment_and_never_retries_an_error() {
    let (screen, mut requests) = fixture(true);
    let session = prepared(&screen, true);
    let mut stream = screen.candidates(&session, "phone").unwrap().boxed();
    let ready = stream.next().await;
    ready.unwrap();
    answer(&screen, &mut requests, &session).await;
    let uploading = tokio::spawn({
        let screen = screen.clone();
        let session = session.clone();
        async move { screen.candidate(&session, "phone", Candidate::Complete).await }
    });
    let request = requests.recv().await;
    let request: Value = serde_json::from_str(&request.unwrap()).unwrap();
    assert!(!uploading.is_finished());
    assert_eq!(request["method"], "candidate");
    assert_eq!(request["params"]["candidate"]["type"], "complete");
    let result = screen.candidate(&session, "phone", Candidate::Complete).await;
    assert!(result.is_err(), "concurrent additions are rejected");
    let link = screen.link().unwrap();
    link.pending
        .locked()
        .remove(&request["id"].as_i64().unwrap())
        .unwrap()
        .send(Err("candidate rejected".into()))
        .unwrap();
    let result = uploading.await;
    assert!(result.unwrap().is_err());
    let first = stream.next().await;
    assert_eq!(first.unwrap()["type"], "error", "the failure overrides buffered helper candidates");
    let result = screen.candidate(&session, "phone", Candidate::Complete).await;
    assert!(result.is_err());
    assert!(requests.try_recv().is_err(), "the uncertain addition was never replayed");
}

#[tokio::test]
async fn disabling_screen_ends_private_signaling_and_rejects_stale_sessions() {
    let (screen, _requests) = fixture(true);
    let session = prepared(&screen, true);
    let mut stream = screen.candidates(&session, "phone").unwrap().boxed();
    let ready = stream.next().await;
    ready.unwrap();
    let directory = std::env::temp_dir().join(format!("codync-screen-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let store = crate::store::Store::open(&directory.join("test.db")).unwrap();
    // No helper is needed once the screen is off; the disconnect path has already stopped media.
    *screen.link.locked() = None;
    let result = screen.set_enabled(&store, false).await;
    result.unwrap();
    assert!(!screen.enabled());
    let ended = stream.next().await;
    assert!(ended.is_none());
    assert!(screen.candidates(&session, "phone").is_err());
}
