//! Construct the host runtime and its owned services.

use super::{Hub, Screen};
use crate::remote::identity::Identity;
use crate::store::Store;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize},
};
use tokio::sync::{broadcast, watch};

impl Hub {
    pub fn new(store: Store, host_id: String, identity: Identity, token: String, port: u16) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        let screen = Arc::new(Screen::new(Screen::load_enabled(&store), events.clone()));
        Arc::new(Self {
            analytics: crate::analytics::Analytics::load(&store),
            routines: crate::routines::Routines::default(),
            team: crate::chat::team::Requests::default(),
            groups: crate::chat::group::Rooms::default(),
            network: crate::network::Network::default(),
            waiting: crate::chat::team::remote::Waiting::default(),
            assistant: crate::chat::team::assistant::Replies::default(),
            live_calls: crate::chat::calls::live::Calls::default(),
            store,
            host_id,
            identity,
            token,
            port,
            events,
            emit_lock: Mutex::new(()),
            bots: Mutex::default(),
            runtime: Mutex::default(),
            phone_clients: AtomicUsize::new(0),
            usage: Mutex::default(),
            keep_awake: Mutex::default(),
            auth: watch::Sender::new(0),
            pairing: Mutex::default(),
            connected: Mutex::default(),
            cloud_synced: AtomicBool::new(false),
            cloud: crate::remote::cloud::Cloud::default(),
            screen,
            terms: Arc::default(),
        })
    }
}
