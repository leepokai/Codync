//! Text pasted from the phone, put on the clipboard of the desktop being shared.

use anyhow::{Result, bail};
use tokio::io::AsyncWriteExt as _;

/// Clipboard writers to try in order, as program and arguments. Sharing X11, only that display's
/// clipboard: it can differ from `DISPLAY` (a helper started without it, or a later xrdp login),
/// and `wl-copy` would reach another desktop. Through the portals, Wayland's clipboard first,
/// then the session's X display.
fn writers(x11_display: Option<&str>) -> Vec<(&str, Vec<&str>)> {
    match x11_display {
        Some(name) => vec![("xclip", vec!["-display", name, "-selection", "clipboard"])],
        None => vec![
            ("wl-copy", vec![]),
            ("xclip", vec!["-selection", "clipboard"]),
        ],
    }
}

/// Puts `text` on the clipboard. `x11_display` is the X display being shared, `None` through
/// the portals.
pub async fn set(text: &str, x11_display: Option<&str>) -> Result<()> {
    let mut started = false;
    for (program, args) in writers(x11_display) {
        let Ok(mut child) = tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        else {
            continue;
        };
        started = true;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await?;
        }
        if child.wait().await?.success() {
            return Ok(());
        }
    }
    match x11_display {
        Some(name) if started => bail!("Couldn't paste into the X display {name}."),
        Some(_) => bail!("Install xclip on this computer to paste from your phone."),
        None => bail!("Install wl-clipboard (or xclip) on this computer to paste from your phone."),
    }
}
