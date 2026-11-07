//! X11 has no consent dialog, so sharing an X display starts only after someone on this computer
//! sets up Computer access (`requestPermission`, which the host takes only from loopback). The
//! choice is kept in the helper's data dir with the display it was made on, and lasts until that
//! file is deleted.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn marker(data: &Path) -> PathBuf {
    data.join("screen-x11-consent")
}

/// The number of the X display setup was done on (`10` for `:10.0`), if it was.
pub fn granted(data: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(marker(data)).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("display="))
        .and_then(|n| n.trim().parse().ok())
}

pub fn grant(data: &Path, display: u32) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = marker(data);
    let text = format!(
        "X11 screen sharing was set up on this computer for display :{display}.\n\
         Delete this file to stop it.\ndisplay={display}\n"
    );
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .with_context(|| format!("couldn't save {}", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// A scratch data dir, removed even when an assertion fails.
    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn consent_names_the_display_it_was_given_on() {
        let dir =
            Dir(std::env::temp_dir().join(format!("codync-x11-consent-{}", std::process::id())));
        std::fs::create_dir_all(&dir.0).unwrap();
        assert_eq!(granted(&dir.0), None);
        grant(&dir.0, 10).unwrap();
        assert_eq!(granted(&dir.0), Some(10));
        grant(&dir.0, 11).unwrap();
        assert_eq!(granted(&dir.0), Some(11));
        // A marker without a display (or edited by hand into nonsense) grants nothing.
        std::fs::write(marker(&dir.0), "yes\n").unwrap();
        assert_eq!(granted(&dir.0), None);
    }
}
