//! Appearance belongs to this terminal client, never the connected host.

use anyhow::Result;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Appearance {
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

fn load() -> Appearance {
    match std::fs::read_to_string(crate::service::data_dir().join("tui-appearance")) {
        Ok(value) => Appearance::parse(&value).unwrap_or(Appearance::System),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::env::var("CODYNC_THEME").ok().and_then(|value| Appearance::parse(&value)).unwrap_or(Appearance::System)
        }
        Err(error) => {
            tracing::warn!(%error, "couldn't read terminal appearance");
            Appearance::System
        }
    }
}

static SELECTED: LazyLock<AtomicU8> = LazyLock::new(|| AtomicU8::new(load() as u8));

pub fn current() -> Appearance {
    match SELECTED.load(Ordering::Relaxed) {
        1 => Appearance::Light,
        2 => Appearance::Dark,
        _ => Appearance::System,
    }
}

pub fn select(mode: Appearance) -> Result<()> {
    let path = crate::service::data_dir().join("tui-appearance");
    std::fs::create_dir_all(crate::service::data_dir())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = std::fs::write(&temporary, mode.name()).and_then(|()| std::fs::rename(&temporary, &path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    SELECTED.store(mode as u8, Ordering::Relaxed);
    Ok(())
}

pub fn system_is_light() -> bool {
    std::env::var("COLORFGBG")
        .ok()
        .and_then(|value| value.rsplit(';').next().and_then(|background| background.parse::<u8>().ok()))
        .is_some_and(|background| background == 7 || background == 15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_choices_round_trip_and_unknown_values_are_rejected() {
        for mode in [Appearance::System, Appearance::Light, Appearance::Dark] {
            assert_eq!(Appearance::parse(mode.name()), Some(mode));
        }
        assert_eq!(Appearance::parse("unknown"), None);
    }
}
