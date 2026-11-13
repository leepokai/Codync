//! The address the host listens on, kept in the data folder rather than in the service's
//! command line: every install and update rewrites that line, so a `--bind` written there
//! by hand was dropped and a host kept on loopback opened to every interface again.

use anyhow::{Context, Result, bail};
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;

const FILE: &str = "bind";

/// All interfaces, so the phone can reach the host directly over the LAN or Tailscale.
const DEFAULT: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// The saved address, or [`DEFAULT`]. A file that doesn't hold an address is an error, not
/// the default: quietly opening a host its owner restricted is what this file prevents.
pub fn load() -> Result<IpAddr> {
    load_from(&super::data_dir())
}

/// Saves `addr` for every later start, including the ones after an update.
pub fn save(addr: IpAddr) -> Result<()> {
    check(addr)?;
    let dir = super::data_dir();
    std::fs::create_dir_all(&dir).context("creating the data directory")?;
    // Written beside it, then renamed over it: a half-written file would keep the host from starting.
    let temporary = dir.join(format!("{FILE}.{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temporary, addr.to_string()).context("saving the bind address")?;
    std::fs::rename(&temporary, dir.join(FILE)).context("saving the bind address")
}

fn load_from(dir: &Path) -> Result<IpAddr> {
    let path = dir.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => text
            .trim()
            .parse()
            .map_err(anyhow::Error::from)
            .and_then(check)
            .with_context(|| format!("{}; set it with `codync-host install --bind <address>`", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Only `127.0.0.1` or `0.0.0.0`: every local client (bots' MCP servers, the desktop app,
/// `status`, the updater's health check) dials `127.0.0.1`, so any other address leaves the
/// host unreachable from its own computer. (`::` isn't dual-stack on Windows.)
fn check(addr: IpAddr) -> Result<IpAddr> {
    if addr == DEFAULT || addr == IpAddr::V4(Ipv4Addr::LOCALHOST) {
        Ok(addr)
    } else {
        bail!("can't listen on {addr}: the host only binds 127.0.0.1 (this computer) or 0.0.0.0 (every interface)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_address_outlives_the_command_line() {
        let dir = std::env::temp_dir().join(format!("codync-bind-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load_from(&dir).unwrap(), DEFAULT, "nothing saved");
        std::fs::write(dir.join(FILE), "127.0.0.1\n").unwrap();
        assert_eq!(load_from(&dir).unwrap(), IpAddr::from([127, 0, 0, 1]));
        std::fs::write(dir.join(FILE), "::1").unwrap();
        assert!(load_from(&dir).is_err(), "local clients dial 127.0.0.1");
        std::fs::write(dir.join(FILE), "not an address").unwrap();
        assert!(load_from(&dir).is_err(), "never falls back to every interface");
        let _ = std::fs::remove_dir_all(dir);
    }
}
