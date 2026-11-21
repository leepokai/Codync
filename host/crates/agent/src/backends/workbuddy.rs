//! `WorkBuddy`'s ACP CLI lives inside the desktop app and is also named `codebuddy`.
//! Looking it up on `PATH` would take `CodeBuddy` Code's binary, so it is found only
//! under a `WorkBuddy` install.

use super::search_path::is_executable;
use std::path::{Path, PathBuf};

/// The app's folder and its CLI's folder in it: macOS, then Windows. The international build
/// (`workbuddy.ai`) is `WorkBuddy AI`; the one for mainland China (`codebuddy.cn`) is `WorkBuddy`.
const APPS_MAC: [&str; 2] = ["WorkBuddy AI.app", "WorkBuddy.app"];
const CLI_DIR_MAC: &str = "Contents/Resources/app.asar.unpacked/cli/bin";
const APPS_OTHER: [&str; 2] = ["WorkBuddy AI", "WorkBuddy"];
const CLI_DIR_OTHER: &str = "resources/app.asar.unpacked/cli/bin";

/// The desktop app ships only `codebuddy-lite-wb.mjs`. `--acp` alone loads the
/// headless bundle, which is not in the app, so the process exits immediately.
const PROCESS_ENV: &[(&str, &str)] = &[("CODEBUDDY_FORCE_LITE_WB_BUNDLE", "1")];
/// The product id in `cli/product.json` when that file can't be read. It is the auth file's name.
const FALLBACK_AUTH_ID: &str = "workbuddy-desktop";

/// A new session proves the agent is signed in. `WorkBuddy` opens one with no token and only fails
/// the first prompt, so its session does not.
pub fn session_proves_sign_in(id: &str) -> bool {
    id != "workbuddy"
}

/// Whether Codync offers harness `id` here: the app is built for macOS and Windows only.
pub(super) fn offered(id: &str) -> bool {
    id != "workbuddy" || cfg!(any(target_os = "macos", windows))
}

/// The bundled CLI is a Node script (`#!/usr/bin/env node`): it runs on the computer's Node, or Codync's.
pub(super) fn runs_on_node(id: &str) -> bool {
    id == "workbuddy"
}

/// `WorkBuddy`'s sign-in logs out when a saved session is already there, and that logout cancels the
/// login it starts next: the auth-file watcher aborts the token poll, then the call waits forever
/// while the browser can already say the login succeeded. Moving the saved session aside first
/// leaves the new process nothing to log out of.
pub fn move_saved_session_aside(id: &str) {
    if id != "workbuddy" {
        return;
    }
    let Some(path) = saved_session_file() else { return };
    retire_file(&path);
}

/// Environment the harness needs in addition to saved sign-in keys. A saved key wins.
pub fn apply_process_env(id: &str, env: &mut Vec<(String, String)>) {
    if id != "workbuddy" {
        return;
    }
    for (key, value) in PROCESS_ENV {
        if env.iter().any(|(name, _)| name == key) {
            continue;
        }
        env.push(((*key).to_owned(), (*value).to_owned()));
    }
}

/// The bundled CLI, when `WorkBuddy` is installed.
pub(super) fn find() -> Option<PathBuf> {
    find_in(&install_roots())
}

/// `program` is that bundled CLI (a path), not the `codebuddy` on `PATH`.
pub(super) fn is_cli(program: &str) -> bool {
    let path = Path::new(program.trim().trim_matches('"'));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !matches!(name, "codebuddy" | "codebuddy.cmd" | "codebuddy.exe") {
        return false;
    }
    let text = path.to_string_lossy().replace('\\', "/");
    APPS_MAC.iter().any(|app| text.contains(&format!("/{app}/{CLI_DIR_MAC}/")))
        || APPS_OTHER.iter().any(|app| text.contains(&format!("/{app}/{CLI_DIR_OTHER}/")))
}

fn cli_dir() -> &'static str {
    if cfg!(target_os = "macos") { CLI_DIR_MAC } else { CLI_DIR_OTHER }
}

fn find_in(roots: &[PathBuf]) -> Option<PathBuf> {
    // Windows runs only these: the extensionless file next to them is the Node script itself.
    let names: &[&str] = if cfg!(windows) { &["codebuddy.cmd", "codebuddy.exe"] } else { &["codebuddy"] };
    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(cli_dir()).join(name)))
        .find(|p| is_executable(p))
}

/// `{home}/Library/Application Support/CodeBuddyExtension/Data/Public/auth/{id}.info` on macOS.
/// The CLI reads this path from the product id (`workbuddy-desktop` for the mainland app).
fn saved_session_file() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let id = product_auth_id().unwrap_or_else(|| FALLBACK_AUTH_ID.to_owned());
    saved_session_path(&home, &id)
}

fn saved_session_path(home: &Path, id: &str) -> Option<PathBuf> {
    if !safe_auth_id(id) {
        return None;
    }
    let base = if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        home.join("AppData").join("Local")
    } else {
        home.join(".local").join("share")
    };
    Some(base.join("CodeBuddyExtension").join("Data").join("Public").join("auth").join(format!("{id}.info")))
}

fn safe_auth_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn product_auth_id() -> Option<String> {
    let bin = find()?;
    let product = bin.parent()?.parent()?.join("product.json");
    let text = std::fs::read_to_string(product).ok()?;
    auth_id_from_product(&text)
}

fn auth_id_from_product(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let id = value.get("authentication")?.get("id")?.as_str()?;
    safe_auth_id(id).then(|| id.to_owned())
}

/// Rename the live auth file. The CLI loads only `{id}.info`, so a `.retired` name is invisible to it.
fn retire_file(path: &Path) {
    if !path.is_file() {
        return;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("session.info");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after the unix epoch")
        .as_millis();
    let backup = path.with_file_name(format!("{name}.{}.{stamp}.retired", std::process::id()));
    match std::fs::rename(path, &backup) {
        Ok(()) => tracing::info!(path = %path.display(), "moved WorkBuddy's saved session aside before sign-in"),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = format!("{error:#}"),
                "couldn't move WorkBuddy's saved session aside"
            );
        }
    }
}

fn install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        let home = dirs::home_dir().map(|home| home.join("Applications"));
        for app in APPS_MAC {
            roots.push(Path::new("/Applications").join(app));
            roots.extend(home.as_ref().map(|dir| dir.join(app)));
        }
    }
    if cfg!(windows) {
        // electron-builder's per-user install, then a machine-wide one.
        let local = dirs::data_local_dir();
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
        for app in APPS_OTHER {
            if let Some(local) = &local {
                roots.push(local.join("Programs").join(app));
                roots.push(local.join(app));
            }
            roots.extend(program_files.as_ref().map(|dir| dir.join(app)));
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_app_bundle_counts_and_a_path_codebuddy_does_not() {
        let apps = if cfg!(target_os = "macos") { APPS_MAC } else { APPS_OTHER };
        for app in apps {
            let dir = std::env::temp_dir().join(format!("codync-workbuddy-{}", uuid::Uuid::new_v4()));
            let root = dir.join(app);
            let bin = root.join(cli_dir()).join(if cfg!(windows) { "codebuddy.cmd" } else { "codebuddy" });
            std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
            std::fs::write(&bin, "#!/bin/sh\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            assert_eq!(find_in(&[root]).as_deref(), Some(bin.as_path()), "{app}");
            assert!(is_cli(&bin.to_string_lossy()), "{app}");
            std::fs::remove_dir_all(dir).unwrap();
        }
        assert!(!is_cli("/opt/homebrew/bin/codebuddy"));
        assert!(!is_cli("/Applications/Other.app/Contents/Resources/app.asar.unpacked/cli/bin/codebuddy"));
    }

    #[test]
    fn the_saved_session_sits_where_the_cli_reads_it() {
        let home = if cfg!(windows) { PathBuf::from(r"C:\Users\example") } else { PathBuf::from("/Users/example") };
        let path = saved_session_path(&home, "workbuddy-desktop").unwrap();
        let base = if cfg!(target_os = "macos") {
            home.join("Library").join("Application Support")
        } else if cfg!(windows) {
            home.join("AppData").join("Local")
        } else {
            home.join(".local").join("share")
        };
        let expected =
            base.join("CodeBuddyExtension").join("Data").join("Public").join("auth").join("workbuddy-desktop.info");
        assert_eq!(path, expected);
        assert!(saved_session_path(&home, "../workbuddy").is_none());
        assert!(saved_session_path(&home, "").is_none());
    }

    #[test]
    fn the_product_auth_id_names_the_session_file() {
        assert_eq!(
            auth_id_from_product(r#"{"authentication":{"id":"workbuddy-desktop"}}"#).as_deref(),
            Some("workbuddy-desktop")
        );
        assert!(auth_id_from_product(r#"{"authentication":{"id":"../x"}}"#).is_none());
        assert!(auth_id_from_product("not json").is_none());
    }

    #[test]
    fn moving_the_saved_session_aside_leaves_the_name_the_cli_loads_empty() {
        let dir = std::env::temp_dir().join(format!("codync-wb-auth-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("workbuddy-desktop.info");
        std::fs::write(&path, "token").unwrap();
        retire_file(&path);
        assert!(!path.exists(), "the cli loads only this exact name");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        assert_eq!(left.len(), 1);
        let name = left[0].to_string_lossy();
        assert!(name.starts_with("workbuddy-desktop.info."), "{name}");
        assert!(name.ends_with(".retired"), "{name}");
        assert_eq!(std::fs::read_to_string(dir.join(&left[0])).unwrap(), "token");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_saved_session_is_left_alone() {
        let path = std::env::temp_dir().join(format!("codync-wb-missing-{}", uuid::Uuid::new_v4()));
        retire_file(&path);
        assert!(!path.exists());
    }
}
