//! Reaching this computer from other devices: identity keys, wire crypto, the
//! E2E channel, the Codync cloud and its relay socket, push, and a cloud computer Codync runs.

pub mod certificate;
pub mod channel;
pub mod claim;
pub mod cloud;
mod cloud_url;
pub mod crypto;
pub mod discovery;
pub mod identity;
pub mod managed;
pub mod owner;
pub mod pairing_requests;
pub mod push;
pub mod relay;
pub mod short_code;
