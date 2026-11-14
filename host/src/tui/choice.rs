//! A choice this terminal keeps for the next run, in `data_dir()/<name>`: never the host's.

use std::io;

/// The saved value; `None` when nothing is saved yet or it can't be read.
pub(in crate::tui) fn read(name: &str) -> Option<String> {
    match std::fs::read_to_string(crate::service::data_dir().join(name)) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(%error, name, "couldn't read the terminal's saved choice");
            None
        }
    }
}

/// Saves `value` whole: written beside the old one, then renamed over it.
pub(in crate::tui) fn write(name: &str, value: &str) -> io::Result<()> {
    let dir = crate::service::data_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = std::fs::write(&temporary, value).and_then(|()| std::fs::rename(&temporary, &path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
