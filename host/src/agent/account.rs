//! Host-owned, isolated Antigravity profiles. Provider tokens stay on this computer.
use crate::agent::{
    acp::{Acp, Incoming},
    auth,
};
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub created_at: i64,
}

pub fn accounts_dir() -> PathBuf {
    crate::service::data_dir().join("accounts")
}

fn checked_id(id: &str) -> Result<()> {
    let valid = id.strip_prefix("acc_").is_some_and(|suffix| {
        (suffix.len() == 8 && suffix.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
            || uuid::Uuid::parse_str(suffix).is_ok_and(|uuid| uuid.to_string() == suffix)
    });
    if !valid {
        bail!("Invalid agent account ID");
    }
    Ok(())
}

fn ordinary(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if metadata.file_type().is_symlink() || reparse {
        bail!("Agent account paths must not contain symbolic links or reparse points");
    }
    Ok(metadata)
}

fn profile_dir(root: &Path, id: &str) -> Result<PathBuf> {
    checked_id(id)?;
    if !ordinary(root)?.is_dir() {
        bail!("Agent account root is not a directory");
    }
    let root = root.canonicalize()?;
    let path = root.join(id);
    if !ordinary(&path)?.is_dir() {
        bail!("Agent account profile is not a directory");
    }
    let path = path.canonicalize()?;
    if path.parent() != Some(root.as_path()) {
        bail!("Agent account profile escaped its root");
    }
    Ok(path)
}

fn read_account(root: &Path, id: &str, backend: Option<&str>) -> Result<AgentAccount> {
    let path = profile_dir(root, id)?.join("account.json");
    if !ordinary(&path)?.is_file() {
        bail!("Agent account metadata is not a file");
    }
    let account: AgentAccount = serde_json::from_slice(&fs::read(path)?).context("Invalid agent account metadata")?;
    if account.id != id || account.backend != "antigravity" || backend.is_some_and(|b| b != account.backend) {
        bail!("Agent account does not match this profile or agent");
    }
    Ok(account)
}

pub fn validate(id: &str, backend: &str) -> Result<AgentAccount> {
    let root = accounts_dir();
    let account = read_account(&root, id, Some(backend))?;
    no_links(&profile_dir(&root, id)?)?;
    Ok(account)
}

pub fn list(backend: Option<&str>) -> Result<Vec<AgentAccount>> {
    let root = accounts_dir();
    if !root.try_exists()? {
        return Ok(Vec::new());
    }
    if !ordinary(&root)?.is_dir() {
        bail!("Agent account root is not a directory");
    }
    let mut accounts = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if checked_id(&id).is_err() {
            continue;
        }
        match read_account(&root, &id, None) {
            Ok(account) if backend.is_none_or(|b| b == account.backend) => accounts.push(account),
            Ok(_) => {}
            Err(error) => tracing::warn!(%id, %error, "skipping invalid or incomplete agent account"),
        }
    }
    accounts.sort_by_key(|a| a.created_at);
    Ok(accounts)
}

pub fn env(id: &str, backend: &str) -> Result<Vec<(String, String)>> {
    validate(id, backend)?;
    Ok(profile_env(&profile_dir(&accounts_dir(), id)?))
}

fn profile_env(path: &Path) -> Vec<(String, String)> {
    vec![
        ("GEMINI_HOME".into(), path.to_string_lossy().into_owned()),
        // The official macOS keychain account is otherwise shared between profiles.
        ("AGY_ACP_FORCE_FILE_STORAGE".into(), "1".into()),
    ]
}

/// Personal OAuth profiles cannot borrow ambient API keys or another credential directory.
pub(super) fn isolate_process_env(process: &mut tokio::process::Command, env: &[(String, String)]) {
    let dedicated = env.iter().any(|(key, value)| key == "GEMINI_HOME" && !value.is_empty())
        && env.iter().any(|(key, value)| key == "AGY_ACP_FORCE_FILE_STORAGE" && value == "1");
    if dedicated {
        for key in [
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GOOGLE_GENAI_USE_VERTEXAI",
            "CLOUDSDK_AUTH_CREDENTIAL_FILE_OVERRIDE",
            "GEMINI_CLI_HOME",
            "GOOGLE_CLOUD_PROJECT",
            "GOOGLE_CLOUD_LOCATION",
            "GOOGLE_CLOUD_QUOTA_PROJECT",
            "GCLOUD_PROJECT",
        ] {
            process.env_remove(key);
        }
    }
}

fn private_directory(path: &Path) -> Result<()> {
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder.create(path)?;
    Ok(())
}

fn create_profile(root: &Path, id: &str) -> Result<PathBuf> {
    checked_id(id)?;
    if !root.try_exists()? {
        private_directory(root)?;
    }
    if !ordinary(root)?.is_dir() {
        bail!("Agent account root is not a directory");
    }
    let path = root.canonicalize()?.join(id);
    private_directory(&path)?;
    profile_dir(root, id)
}

fn write_account(path: &Path, account: &AgentAccount) -> Result<()> {
    write_private_json(&path.join("account.json"), account)
}

fn write_private_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(&serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn configure_personal_auth(path: &Path) -> Result<()> {
    let settings = path.join("antigravity-acp");
    private_directory(&settings)?;
    write_private_json(&settings.join("settings.json"), &json!({"auth": {"type": "oauth-personal"}}))
}

fn personal_auth_method(initialize: &serde_json::Value) -> Result<&'static str> {
    if initialize["authMethods"].as_array().is_some_and(|methods| methods.iter().any(|m| m["id"] == "oauth-personal")) {
        Ok("oauth-personal")
    } else {
        bail!(
            "This Antigravity adapter does not advertise personal Google OAuth. Update the adapter before adding an account."
        )
    }
}

fn no_links(path: &Path) -> Result<()> {
    if ordinary(path)?.is_dir() {
        for entry in fs::read_dir(path)? {
            no_links(&entry?.path())?;
        }
    }
    Ok(())
}

fn remove_profile(root: &Path, id: &str) -> Result<()> {
    let path = profile_dir(root, id)?;
    no_links(&path)?;
    fs::remove_dir_all(path)?;
    Ok(())
}

struct PendingProfile {
    root: PathBuf,
    id: String,
    acp: Option<Arc<Acp>>,
    committed: bool,
}

impl Drop for PendingProfile {
    fn drop(&mut self) {
        let acp = self.acp.take();
        let root = self.root.clone();
        let id = self.id.clone();
        let remove = !self.committed;
        let cleanup = move || {
            if remove && let Err(error) = remove_profile(&root, &id) {
                tracing::warn!(%id, %error, "could not remove unfinished agent account");
            }
        };
        if let Some(acp) = acp {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    acp.kill_and_wait().await;
                    if let Err(error) = tokio::task::spawn_blocking(cleanup).await {
                        tracing::warn!(%error, "agent account cleanup failed");
                    }
                });
            } else {
                drop(acp);
                cleanup();
            }
        } else {
            cleanup();
        }
    }
}

pub async fn authenticate(store: &Store, backend: &str, name: Option<String>) -> Result<AgentAccount> {
    tokio::time::timeout(Duration::from_mins(10), authenticate_profile(store, backend, name))
        .await
        .context("Sign-in didn't finish within 10 minutes")?
}

async fn authenticate_profile(store: &Store, backend: &str, name: Option<String>) -> Result<AgentAccount> {
    if backend != "antigravity" {
        bail!("Account login is currently supported for Google Antigravity");
    }
    let id = format!("acc_{}", uuid::Uuid::new_v4());
    let root = accounts_dir();
    let path = create_profile(&root, &id)?;
    let mut pending = PendingProfile { root, id: id.clone(), acp: None, committed: false };
    configure_personal_auth(&path)?;
    let (acp, mut inbox, initialize, _) = auth::start_with_env(store, backend, &profile_env(&path)).await?;
    pending.acp = Some(acp.clone());
    let method = personal_auth_method(&initialize)?;
    let result = async {
        let request = acp.request("authenticate", json!({"methodId": method}));
        tokio::pin!(request);
        loop {
            tokio::select! {
                result = &mut request => break result.context("signing in to account"),
                incoming = inbox.recv() => match incoming {
                    Some(Incoming::Request { id, .. }) => acp.respond_error(id, -32601, "not supported during account sign-in").await?,
                    Some(Incoming::Notification { .. }) => {},
                    Some(Incoming::Closed { .. }) | None => bail!("Agent closed during account sign-in"),
                }
            }
        }
    }.await;
    acp.kill_and_wait().await;
    pending.acp = None;
    result?;
    let fallback_name = format!("Google account ({})", &id[id.len() - 8..]);
    let account = AgentAccount {
        id,
        name: name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).unwrap_or(fallback_name),
        backend: backend.into(),
        created_at: crate::store::now_ms(),
    };
    write_account(&path, &account)?;
    pending.committed = true;
    Ok(account)
}

pub fn delete(store: &Store, id: &str) -> Result<()> {
    checked_id(id)?;
    if store.bots()?.iter().any(|bot| !bot.deleted && bot.config.account.as_deref() == Some(id)) {
        bail!("This account is used by a bot. Change that bot's account before deleting it.");
    }
    read_account(&accounts_dir(), id, None)?;
    remove_profile(&accounts_dir(), id)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Profiles(PathBuf);
    impl Profiles {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("codync-account-test-{}", uuid::Uuid::new_v4()));
            private_directory(&path).unwrap();
            Self(path)
        }
        fn add(&self, id: &str) -> PathBuf {
            let path = create_profile(&self.0, id).unwrap();
            write_account(
                &path,
                &AgentAccount { id: id.into(), name: "Test".into(), backend: "antigravity".into(), created_at: 0 },
            )
            .unwrap();
            path
        }
    }
    impl Drop for Profiles {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn dedicated_process_removes_saved_and_inherited_credential_overrides() {
        let mut process = tokio::process::Command::new("unused-test-executable");
        let mut env = profile_env(Path::new("isolated-profile"));
        env.push(("GEMINI_API_KEY".into(), "saved-key".into()));
        process.envs(env.iter().map(|(key, value)| (key, value)));
        isolate_process_env(&mut process, &env);
        let vars: std::collections::BTreeMap<_, _> = process.as_std().get_envs().collect();
        for key in [
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GOOGLE_GENAI_USE_VERTEXAI",
            "CLOUDSDK_AUTH_CREDENTIAL_FILE_OVERRIDE",
            "GEMINI_CLI_HOME",
        ] {
            assert_eq!(vars.get(std::ffi::OsStr::new(key)), Some(&None), "must remove {key}");
        }
        assert_eq!(
            vars.get(std::ffi::OsStr::new("AGY_ACP_FORCE_FILE_STORAGE")),
            Some(&Some(std::ffi::OsStr::new("1")))
        );
    }

    #[test]
    fn default_process_keeps_its_configured_provider_credentials() {
        let mut process = tokio::process::Command::new("unused-test-executable");
        let env = vec![("GEMINI_API_KEY".into(), "configured-key".into())];
        process.envs(env.iter().map(|(key, value)| (key, value)));
        isolate_process_env(&mut process, &env);
        let vars: std::collections::BTreeMap<_, _> = process.as_std().get_envs().collect();
        assert_eq!(vars.len(), 1);
        assert_eq!(
            vars.get(std::ffi::OsStr::new("GEMINI_API_KEY")),
            Some(&Some(std::ffi::OsStr::new("configured-key")))
        );
    }

    #[test]
    fn dedicated_profiles_select_personal_oauth_and_file_credentials() {
        let profiles = Profiles::new();
        let path = create_profile(&profiles.0, "acc_deadbeef").unwrap();
        configure_personal_auth(&path).unwrap();
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("antigravity-acp/settings.json")).unwrap()).unwrap();
        assert_eq!(settings["auth"]["type"], "oauth-personal");
        let vars = profile_env(&path);
        assert!(vars.iter().any(|(key, value)| key == "GEMINI_HOME" && Path::new(value) == path));
        assert!(vars.iter().any(|(key, value)| key == "AGY_ACP_FORCE_FILE_STORAGE" && value == "1"));
        assert!(configure_personal_auth(&path).is_err(), "must not overwrite existing profile settings");
    }

    #[test]
    fn login_requires_the_adapter_to_advertise_personal_oauth() {
        assert_eq!(
            personal_auth_method(&json!({"authMethods": [{"id": "oauth-personal"}]})).unwrap(),
            "oauth-personal"
        );
        assert!(personal_auth_method(&json!({"authMethods": [{"id": "gemini-api-key"}]})).is_err());
        assert!(personal_auth_method(&json!({})).is_err());
    }

    #[test]
    fn account_ids_cannot_escape_the_profile_root() {
        for id in [
            "../outside",
            "acc_../outside",
            "C:\\Windows",
            "/tmp",
            "acc_12345678/..",
            "acc_ABCDEF00",
            "acc_",
            "acc_deadbeef:stream",
        ] {
            assert!(checked_id(id).is_err(), "accepted {id}");
        }
        assert!(checked_id("acc_deadbeef").is_ok());
        assert!(checked_id(&format!("acc_{}", uuid::Uuid::new_v4())).is_ok());
    }
    #[test]
    fn account_metadata_must_match_directory_and_backend() {
        let profiles = Profiles::new();
        let path = profiles.add("acc_deadbeef");
        assert!(read_account(&profiles.0, "acc_deadbeef", Some("antigravity")).is_ok());
        assert!(read_account(&profiles.0, "acc_deadbeef", Some("claude")).is_err());
        fs::write(
            path.join("account.json"),
            r#"{"id":"acc_12345678","name":"Test","backend":"antigravity","createdAt":0}"#,
        )
        .unwrap();
        assert!(read_account(&profiles.0, "acc_deadbeef", None).is_err());
    }
    #[test]
    fn creation_is_exclusive_and_removal_is_scoped() {
        let profiles = Profiles::new();
        profiles.add("acc_deadbeef");
        let other = profiles.add("acc_12345678");
        assert!(create_profile(&profiles.0, "acc_deadbeef").is_err());
        remove_profile(&profiles.0, "acc_deadbeef").unwrap();
        assert!(other.exists());
        assert!(remove_profile(&profiles.0, "../outside").is_err());
    }
    #[test]
    fn failed_signin_guard_removes_uncommitted_profile() {
        let profiles = Profiles::new();
        let id = "acc_deadbeef";
        let path = create_profile(&profiles.0, id).unwrap();
        {
            let _pending = PendingProfile { root: profiles.0.clone(), id: id.into(), acp: None, committed: false };
        }
        assert!(!path.exists());
    }
    #[cfg(unix)]
    #[test]
    fn links_are_rejected_and_tokens_have_private_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let profiles = Profiles::new();
        let path = profiles.add("acc_deadbeef");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(path.join("account.json")).unwrap().permissions().mode() & 0o777, 0o600);
        symlink(&path, profiles.0.join("acc_12345678")).unwrap();
        assert!(read_account(&profiles.0, "acc_12345678", None).is_err());
        assert!(remove_profile(&profiles.0, "acc_12345678").is_err());
        fs::remove_file(profiles.0.join("acc_12345678")).unwrap();
    }
}
