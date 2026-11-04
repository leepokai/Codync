//! Device-only folding preferences keyed by the authenticated computer identity.
use super::App;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf};

#[derive(Default, Serialize, Deserialize)]
struct Presentation {
    by_project: bool,
    collapsed: HashSet<String>,
}

fn path(computer: &str) -> Result<PathBuf> {
    use std::fmt::Write as _;
    let digest = Sha256::digest(computer.as_bytes()).iter().fold(String::with_capacity(64), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    });
    let name = format!("{digest}.json");
    Ok(dirs::config_dir()
        .context("device preferences directory is unavailable")?
        .join("Codync")
        .join("tui-roster")
        .join(name))
}

impl App {
    pub(super) fn load_roster_preferences(&mut self, computer: &str) {
        if self.demo {
            return;
        }
        if self.roster_tools.preferences_context.as_deref() == Some(computer) {
            return;
        }
        self.roster_tools.preferences_context = Some(computer.to_owned());
        let loaded = (|| -> Result<Presentation> {
            match std::fs::read(path(computer)?) {
                Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Presentation::default()),
                Err(error) => Err(error.into()),
            }
        })();
        match loaded {
            Ok(value) => {
                self.roster_tools.by_project = value.by_project;
                self.roster_tools.collapsed = value.collapsed;
            }
            Err(error) => self.flash(&format!("Couldn't load roster preferences: {error:#}")),
        }
    }

    pub(super) fn save_roster_preferences(&mut self) {
        if self.demo {
            return;
        }
        let Some(computer) = &self.roster_tools.preferences_context else { return };
        let saved = (|| -> Result<()> {
            let target = path(computer)?;
            std::fs::create_dir_all(target.parent().context("missing preferences directory")?)?;
            let value = Presentation {
                by_project: self.roster_tools.by_project,
                collapsed: self.roster_tools.collapsed.clone(),
            };
            let temporary = target.with_extension(format!("{}.tmp", std::process::id()));
            std::fs::write(&temporary, serde_json::to_vec(&value)?)?;
            std::fs::rename(temporary, target)?;
            Ok(())
        })();
        if let Err(error) = saved {
            self.flash(&format!("Couldn't save roster preferences: {error:#}"));
        }
    }
}
