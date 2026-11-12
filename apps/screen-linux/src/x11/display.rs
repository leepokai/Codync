//! Which X display to share. Never one in a Wayland session, whose `DISPLAY` is Xwayland and
//! shows only X clients; `X11::connect` also turns down any server that is Xwayland.

use std::ffi::OsString;

/// X displays to try, best first: `$DISPLAY`, then this user's own X servers (a service started
/// without `DISPLAY`, or one whose display went away when xrdp gave the next login `:11`).
/// Empty in a Wayland session. The servers come from `/tmp/.X11-unix`, sorted as the host
/// sorts them for `cua-driver`, unless Sway or a Wayland socket says the desktop is Wayland.
pub fn candidates() -> Vec<String> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let session_wayland = env("WAYLAND_DISPLAY").is_some()
        || env("SWAYSOCK").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
    let service_wayland = || {
        env("XDG_RUNTIME_DIR").is_some_and(|dir| {
            std::fs::read_dir(dir).is_ok_and(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("wayland-"))
            })
        })
    };
    choose(
        session_wayland,
        std::env::var("DISPLAY").ok(),
        service_wayland,
        own_x_servers,
    )
}

/// The displays setup may choose from. Without `DISPLAY` and with several X servers it can't
/// tell which desktop the person setting up is on, so it refuses instead of guessing.
pub fn for_setup() -> anyhow::Result<Vec<String>> {
    let found = candidates();
    let explicit = std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty());
    if !explicit && found.len() > 1 {
        anyhow::bail!(
            "several X displays ({}) and no DISPLAY: start codync-host with DISPLAY set to the one to share",
            found.join(", ")
        );
    }
    Ok(found)
}

/// The display number in `:10.0` or `host:10`.
pub fn number(name: &str) -> Option<u32> {
    let (_, rest) = name.rsplit_once(':')?;
    rest.split('.').next()?.parse().ok()
}

/// Whether the helper may pick `name` on its own, without a new setup: the display setup was done
/// on, or a later one (xrdp and VNC number each new session above the last). An earlier display,
/// such as a console session on `:0`, is another desktop and needs its own setup.
pub fn follows_consent(name: &str, granted: u32) -> bool {
    number(name).is_some_and(|n| n >= granted)
}

/// Sockets in `/tmp/.X11-unix` owned by this user, as `:N`.
fn own_x_servers() -> Vec<String> {
    use std::os::unix::fs::MetadataExt as _;
    let Ok(me) = std::fs::metadata("/proc/self").map(|m| m.uid()) else {
        return Vec::new();
    };
    let sockets = std::fs::read_dir("/tmp/.X11-unix")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.file_name(), e.metadata().ok()?.uid())));
    servers_owned_by(sockets, me)
}

/// `:N` for each `XN` socket that `uid` owns: another user's X server is their desktop.
fn servers_owned_by(sockets: impl IntoIterator<Item = (OsString, u32)>, uid: u32) -> Vec<String> {
    sockets
        .into_iter()
        .filter(|(_, owner)| *owner == uid)
        .filter_map(|(name, _)| Some(format!(":{}", name.to_str()?.strip_prefix('X')?)))
        .collect()
}

fn choose(
    session_wayland: bool,
    display: Option<String>,
    service_wayland: impl FnOnce() -> bool,
    servers: impl FnOnce() -> Vec<String>,
) -> Vec<String> {
    if session_wayland {
        return Vec::new();
    }
    let display = display.filter(|d| !d.is_empty());
    let mut found = Vec::new();
    if !service_wayland() {
        found = servers();
        found.sort();
    }
    // `:10.0` and `:10` are the same server.
    if let Some(display) = display {
        found.retain(|s| number(s) != number(&display));
        found.insert(0, display);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_comes_first_then_own_servers_and_never_in_wayland() {
        let (no, yes) = (|| false, || true);
        let none = Vec::new;
        let servers = || vec![":11".to_owned(), ":10".to_owned()];
        assert_eq!(choose(false, Some(":10.0".into()), no, none), [":10.0"]);
        // After a relogin, xrdp's next display is the fallback when `$DISPLAY` is gone.
        assert_eq!(
            choose(false, Some(":10.0".into()), no, servers),
            [":10.0", ":11"]
        );
        // A Wayland socket elsewhere (a console session) doesn't hide an explicit X session,
        // but its X servers aren't guessed.
        assert_eq!(choose(false, Some(":10.0".into()), yes, servers), [":10.0"]);
        assert_eq!(choose(false, None, no, servers), [":10", ":11"]);
        assert_eq!(
            choose(false, Some(String::new()), no, servers),
            [":10", ":11"]
        );
        assert_eq!(choose(false, None, yes, servers), Vec::<String>::new());
        assert_eq!(choose(false, None, no, none), Vec::<String>::new());
        // Wayland, Sway included: not even an explicit `DISPLAY`.
        assert_eq!(
            choose(true, Some(":0".into()), no, servers),
            Vec::<String>::new()
        );
    }

    #[test]
    fn only_the_consented_display_or_a_later_one_is_picked_automatically() {
        assert_eq!(number(":10.0"), Some(10));
        assert_eq!(number(":11"), Some(11));
        assert_eq!(number("localhost:12.0"), Some(12));
        assert_eq!(number("nonsense"), None);
        assert!(follows_consent(":10.0", 10));
        assert!(follows_consent(":11", 10));
        assert!(!follows_consent(":0", 10));
        assert!(!follows_consent(":9", 10));
        assert!(!follows_consent("nonsense", 0));
    }

    #[test]
    fn only_this_users_x_servers_count() {
        let sockets = [
            (OsString::from("X10"), 1001),
            (OsString::from("X11"), 1002),
            (OsString::from("X21"), 1002),
            (OsString::from("X12"), 1001),
            (OsString::from("lock"), 1001),
        ];
        assert_eq!(servers_owned_by(sockets.clone(), 1001), [":10", ":12"]);
        assert_eq!(servers_owned_by(sockets, 1002), [":11", ":21"]);
    }
}
