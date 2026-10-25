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
}
