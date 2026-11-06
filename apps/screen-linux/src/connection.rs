//! Native ICE state notifications run on GStreamer's threads, outside Tokio.

use crate::Helper;
use gst::prelude::*;
use std::sync::Arc;

pub(super) fn observe(peer: &gst::Element, id: String, helper: &Arc<Helper>) {
    let runtime = tokio::runtime::Handle::current();
    let helper = Arc::downgrade(helper);
    peer.connect_notify(Some("ice-connection-state"), move |peer, _| {
        let state = peer.property::<gst_webrtc::WebRTCICEConnectionState>("ice-connection-state");
        let Some(helper) = helper.upgrade() else {
            return;
        };
        let id = id.clone();
        let peer = peer.clone();
        runtime.spawn(async move { changed(&helper, &id, &peer, state).await });
    });
}

async fn changed(
    helper: &Arc<Helper>,
    id: &str,
    peer: &gst::Element,
    state: gst_webrtc::WebRTCICEConnectionState,
) {
    use gst_webrtc::WebRTCICEConnectionState as Ice;
    match state {
        Ice::Connected | Ice::Completed => helper.connected(id, peer).await,
        Ice::Failed | Ice::Closed => helper.ended_peer(id, peer).await,
        // A disconnected connection may recover without renegotiating.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::stream::Session;
    use tokio::sync::{Mutex, mpsc};

    fn session() -> Session {
        gst::init().unwrap();
        Session {
            pipeline: gst::Pipeline::new(),
            webrtc: gst::ElementFactory::make("webrtcbin").build().unwrap(),
            incoming: Some(std::sync::Mutex::default()),
        }
    }

    #[tokio::test]
    async fn connection_notifications_ignore_closed_and_replaced_native_peers() {
        let (out, mut events) = mpsc::unbounded_channel();
        let helper = Arc::new(Helper {
            portal: Arc::new(Mutex::new(None)),
            last_status: Mutex::new(None),
            sessions: crate::sessions::Sessions::default(),
            out,
            rt: tokio::runtime::Handle::current(),
        });
        let original = session();
        let old_peer = original.webrtc.clone();
        let answer = helper
            .sessions
            .answer("viewer".into(), "offer", async {
                Ok((original, "answer".into()))
            })
            .await;
        answer.unwrap();
        changed(
            &helper,
            "viewer",
            &old_peer,
            gst_webrtc::WebRTCICEConnectionState::Connected,
        )
        .await;
        let connected = events.try_recv().unwrap();
        assert_eq!(connected["method"], "session");
        assert_eq!(connected["params"]["state"], "connected");
        changed(
            &helper,
            "viewer",
            &old_peer,
            gst_webrtc::WebRTCICEConnectionState::Disconnected,
        )
        .await;
        assert!(
            events.try_recv().is_err(),
            "temporary ICE disconnects keep media alive"
        );
        helper.sessions.close("viewer").await;
        changed(
            &helper,
            "viewer",
            &old_peer,
            gst_webrtc::WebRTCICEConnectionState::Completed,
        )
        .await;
        assert!(events.try_recv().is_err());
        let replacement = session();
        let peer = replacement.webrtc.clone();
        let answer = helper
            .sessions
            .answer("viewer".into(), "offer", async {
                Ok((replacement, "answer".into()))
            })
            .await;
        answer.unwrap();
        changed(
            &helper,
            "viewer",
            &old_peer,
            gst_webrtc::WebRTCICEConnectionState::Connected,
        )
        .await;
        assert!(
            events.try_recv().is_err(),
            "late callbacks belong only to their native peer"
        );
        changed(
            &helper,
            "viewer",
            &old_peer,
            gst_webrtc::WebRTCICEConnectionState::Failed,
        )
        .await;
        let still_live = helper.sessions.is_live("viewer", &peer).await;
        assert!(still_live, "an old peer cannot close its replacement");
        changed(
            &helper,
            "viewer",
            &old_peer,
            gst_webrtc::WebRTCICEConnectionState::Closed,
        )
        .await;
        let still_live = helper.sessions.is_live("viewer", &peer).await;
        assert!(still_live);
        assert!(events.try_recv().is_err());
        changed(
            &helper,
            "viewer",
            &peer,
            gst_webrtc::WebRTCICEConnectionState::Completed,
        )
        .await;
        assert_eq!(events.try_recv().unwrap()["params"]["state"], "connected");
        changed(
            &helper,
            "viewer",
            &peer,
            gst_webrtc::WebRTCICEConnectionState::Closed,
        )
        .await;
        assert_eq!(events.try_recv().unwrap()["params"]["state"], "closed");
        let still_live = helper.sessions.is_live("viewer", &peer).await;
        assert!(!still_live);
    }
}
