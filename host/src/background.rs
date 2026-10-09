//! Background services started alongside the host API.

use crate::{
    agent::{backends, registry},
    hub::Hub,
    market,
    remote::relay,
    screen, update, usage, voice,
};
use std::sync::Arc;

pub fn start(hub: &Arc<Hub>) {
    tokio::spawn(registry::refresh_loop());
    tokio::spawn(market::refresh_first_page());
    tokio::spawn(market::automatic_loop(hub.clone()));
    tokio::spawn(voice::refresh_loop(hub.clone()));
    tokio::spawn(backends::refresh_sign_in());
    tokio::spawn(usage::poll(hub.clone()));
    tokio::spawn(update::automatic_loop(hub.clone()));
    tokio::spawn(screen::serve_helpers(hub.screen.clone()));
    tokio::spawn(relay::run(hub.clone()));
    #[cfg(target_os = "linux")]
    tokio::spawn(screen::supervise_linux_helper(hub.screen.clone()));
}
