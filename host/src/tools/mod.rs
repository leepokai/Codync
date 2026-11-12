//! Bundled tools: programs that ship inside the app's resources, run as one local process on one
//! port and keep their data in the user's folder (see `docs/plans/bundled-tools-production.md`).
//!
//! This is the single place engines are declared. A new engine adds an [`Engine`] here and a launch
//! function; the runner, the health probe and the lifecycle are shared. n8n predates this module and
//! still runs through `crate::n8n`.

pub mod comfy;
mod comfy_api;
pub(crate) mod comfy_install;
mod comfy_job;
pub mod comfy_mcp;
mod health;
mod lifecycle;
mod log;
pub mod runner;

#[cfg(test)]
mod tests;

pub use lifecycle::Tools;

use anyhow::Result;
use runner::Launch;
use std::time::Duration;

/// One engine: what the shared runner needs to start it and know it is ready.
pub struct Engine {
    pub id: &'static str,
    pub label: &'static str,
    pub default_port: u16,
    /// A GET on this path answers 2xx once the engine accepts work.
    pub health_path: &'static str,
    /// How long a start may take before it counts as failed (the first start of a big engine is slow).
    pub ready_timeout: Duration,
    /// The program and arguments for a start on `port`.
    pub launch: fn(port: u16) -> Result<Launch>,
}

/// ComfyUI: the official portable, started as it is; see [`comfy`].
pub static COMFY: Engine = Engine {
    id: "comfy",
    label: "ComfyUI",
    default_port: 8188,
    health_path: "/system_stats",
    ready_timeout: Duration::from_secs(300),
    launch: comfy::launch,
};

pub static ENGINES: [&Engine; 1] = [&COMFY];
