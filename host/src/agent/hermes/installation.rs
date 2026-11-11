//! A source install and its own PM interpreter. Never borrow an arbitrary system Python.

use super::Settings;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const REVISION: &str = "10d25f8ad65fdf71304e9272959421a0c22e5d23";

pub(crate) struct Installation {
    pub root: PathBuf,
    pub python: PathBuf,
}

fn interpreter(venv: &Path) -> PathBuf {
    venv.join(if cfg!(windows) { "Scripts/python.exe" } else { "bin/python" })
}

fn roots(config: &Settings) -> Vec<PathBuf> {
    if !config.root.trim().is_empty() {
        return vec![PathBuf::from(&config.root)];
    }
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HERMES_HOME") {
        roots.push(PathBuf::from(home).join("hermes-agent"));
    }
    if let Some(local) = dirs::data_local_dir() {
        roots.push(local.join("hermes/hermes-agent"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".hermes/hermes-agent"));
    }
    roots
}

fn python(root: &Path, config: &Settings) -> Result<PathBuf> {
    if !config.python.trim().is_empty() {
        let path = PathBuf::from(&config.python);
        ensure!(path.is_absolute() && path.is_file(), "Choose an absolute path to Hermes's interpreter");
        ensure!(
            path.parent().and_then(Path::parent).is_some_and(|venv| venv.join("pyvenv.cfg").is_file()),
            "Choose Hermes's own virtual-environment interpreter"
        );
        return Ok(path);
    }
    // Match PM's install_key; strip only Windows' canonical extended-path prefix.
    let canonical = root.to_string_lossy();
    let key = canonical.strip_prefix(r"\\?\").unwrap_or(&canonical);
    let key = crate::chat::files::digest_hex(&Sha256::digest(key.as_bytes()));
    let owner = root.parent().context("Hermes source has no owning directory")?;
    let state = owner.join("installs").join(&key[..16]);
    let facts = state.join("facts.json");
    if facts.is_file() {
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&facts)?).context("Hermes PM environment record is invalid")?;
        if let Some(environment) = doc["packages"]["venv"]["environment"].as_str() {
            let environment = PathBuf::from(environment).canonicalize()?;
            let generations = state.join("environments").canonicalize()?;
            ensure!(
                environment.starts_with(generations) && environment.join("pyvenv.cfg").is_file(),
                "Hermes PM environment is outside its install"
            );
            let executable = interpreter(&environment);
            ensure!(executable.is_file(), "Hermes PM interpreter is missing; repair this Hermes installation");
            return Ok(executable);
        }
    }
    for venv in [root.join("venv"), root.join(".venv")] {
        let path = interpreter(&venv);
        if venv.join("pyvenv.cfg").is_file() && path.is_file() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "Hermes's own Python environment was not found. Install the pinned Hermes runtime or choose its interpreter in bot settings."
    )
}

async fn git(root: &Path, args: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = tokio::process::Command::new(
        crate::agent::backends::which("git").context("Git is required to verify the Hermes source revision")?,
    );
    cmd.arg("-C").arg(root).args(args);
    tokio::time::timeout(Duration::from_secs(8), crate::agent::process::output::run(cmd, 16 * 1024))
        .await
        .context("Hermes source verification timed out")?
}

impl Installation {
    pub(crate) async fn profile_metadata(&self, env: &[(String, String)]) -> Result<serde_json::Value> {
        let owner = self.prepare_owner().await?;
        let mut command = tokio::process::Command::new(&self.python);
        command.arg("-B").arg(owner.with_file_name("state_probe.py")).arg("--hermes-root").arg(&self.root);
        for (key, value) in env {
            if matches!(key.as_str(), "HERMES_HOME" | "HERMES_PROFILE" | "HERMES_PROFILE_NAME") {
                command.env(key, value);
            }
        }
        let result =
            tokio::time::timeout(Duration::from_secs(10), crate::agent::process::output::run(command, 32 * 1024))
                .await
                .context("Hermes profile inspection timed out")??;
        ensure!(result["success"] == true, "Hermes profile metadata could not be read");
        serde_json::from_str(result["stdout"].as_str().context("Hermes profile inspection returned no metadata")?)
            .context("Hermes profile metadata is invalid")
    }

    pub(crate) async fn resolve(config: &Settings) -> Result<Self> {
        let root = roots(config).into_iter().find(|path| path.join("acp_adapter/server.py").is_file())
            .context("Pinned Hermes source install is missing; install Hermes or choose its source directory in bot settings")?
            .canonicalize()?;
        let revision = git(&root, &["rev-parse", "HEAD"]).await?;
        ensure!(
            revision["success"] == true && revision["stdout"].as_str().is_some_and(|s| s.trim() == REVISION),
            "Hermes must remain at revision {REVISION}"
        );
        let clean = git(&root, &["diff", "--quiet", "HEAD", "--"]).await?;
        ensure!(
            clean["success"] == true,
            "Hermes source contains tracked edits; use the unchanged pinned installation"
        );
        let python = python(&root, config)?;
        let venv = python.parent().and_then(Path::parent).context("Hermes interpreter has no environment")?;
        let metadata = std::fs::read_to_string(venv.join("pyvenv.cfg"))?;
        let version = metadata
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "version").then(|| value.trim())
            })
            .context("Hermes environment has no Python version metadata")?;
        let mut parts = version.split('.');
        ensure!(
            parts.next() == Some("3")
                && parts
                    .next()
                    .and_then(|part| part.parse::<u8>().ok())
                    .is_some_and(|minor| (11..=14).contains(&minor)),
            "Hermes requires its own Python 3.11–3.14 environment"
        );
        Ok(Self { root, python })
    }

    pub(crate) async fn prepare_owner(&self) -> Result<PathBuf> {
        let source = [
            ("HERMES-LICENSE", include_str!("../../market/hermes/LICENSE")),
            ("acp_owner.py", include_str!("acp_owner.py")),
            ("state_probe.py", include_str!("state_probe.py")),
            ("environment_probe.py", include_str!("environment_probe.py")),
            ("delegation_routes.py", include_str!("delegation_routes.py")),
            ("delegation_quota.py", include_str!("delegation_quota.py")),
            ("ssh_owner.py", include_str!("ssh_owner.py")),
            ("ssh_workspace.py", include_str!("ssh_workspace.py")),
            ("ssh_processes.py", include_str!("ssh_processes.py")),
        ];
        let mut digest = Sha256::new();
        for (name, content) in &source {
            digest.update(name.as_bytes());
            digest.update(content.len().to_le_bytes());
            digest.update(content.as_bytes());
        }
        let digest = digest.finalize();
        let root = crate::service::data_dir().join("hermes-runtime").join(crate::chat::files::digest_hex(&digest));
        tokio::fs::create_dir_all(&root).await?;
        for (name, content) in source {
            let path = root.join(name);
            if tokio::fs::read(&path).await.ok().as_deref() != Some(content.as_bytes()) {
                let temporary = root.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
                tokio::fs::write(&temporary, content).await?;
                if let Err(error) = tokio::fs::rename(&temporary, &path).await {
                    let equal = tokio::fs::read(&path).await.ok().as_deref() == Some(content.as_bytes());
                    let _ = tokio::fs::remove_file(&temporary).await;
                    ensure!(equal, "Could not publish Hermes bridge: {error}");
                }
            }
        }
        Ok(root.join("acp_owner.py"))
    }
}
