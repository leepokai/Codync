//! ComfyUI as a bundled tool: where its files are, where its data lives, how it starts.
//!
//! The program is the official Windows portable (`python_embeded/` + `ComfyUI/`), unpacked by the
//! packaging step into the app's resources and started as it is (`python main.py`), never edited.
//! Everything that changes (settings, models, custom nodes, inputs, outputs) lives under
//! `<data>/comfy`, so replacing the program on an update keeps it all.
//! Source: Comfy-Org/ComfyUI at the commit pinned in `packaging/comfy/version.json`; the flags used
//! here are those of `comfy/cli_args.py` and the folder keys those of `extra_model_paths.yaml.example`.

use super::runner::Launch;
use crate::LockExt;
use crate::service;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The id of the connector that turns the `comfyui` MCP tools on for a bot.
pub const CONNECTOR_ID: &str = "comfyui";
/// Free space the first start wants: the program unpacks Python packages and the first models are big.
const MIN_FREE: u64 = 10 * 1024 * 1024 * 1024;
/// Where a missing checkpoint can be downloaded (the official model list).
pub const MODELS_URL: &str = "https://github.com/Comfy-Org/ComfyUI?tab=readme-ov-file#manual-install-windows-linux";

/// The one place the bundled version is named; the packaging script reads the same file.
#[derive(Deserialize)]
pub struct Pin {
    pub version: String,
    pub commit: String,
}

pub fn pin() -> &'static Pin {
    static PIN: LazyLock<Pin> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../packaging/comfy/version.json"))
            .expect("packaging/comfy/version.json is valid JSON with version and commit")
    });
    &PIN
}

/// Model folders ComfyUI is pointed at, relative to [`data_dir`] (keys of `extra_model_paths.yaml.example`).
const MODEL_FOLDERS: [(&str, &str); 12] = [
    ("checkpoints", "models/checkpoints/"),
    ("configs", "models/configs/"),
    ("loras", "models/loras/"),
    ("vae", "models/vae/"),
    ("text_encoders", "models/text_encoders/"),
    ("diffusion_models", "models/diffusion_models/"),
    ("clip_vision", "models/clip_vision/"),
    ("embeddings", "models/embeddings/"),
    ("controlnet", "models/controlnet/"),
    ("upscale_models", "models/upscale_models/"),
    ("vae_approx", "models/vae_approx/"),
    ("custom_nodes", "custom_nodes/"),
];

/// The user's ComfyUI data; it survives updates and uninstalls that keep data.
pub fn data_dir() -> PathBuf {
    service::data_dir().join("comfy")
}

pub fn checkpoints_dir() -> PathBuf {
    data_dir().join("models").join("checkpoints")
}

pub fn output_dir() -> PathBuf {
    data_dir().join("output")
}

/// The unpacked portable: `CODYNC_COMFY_DIR` for development, else `comfy/` beside the host.
pub fn program_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CODYNC_COMFY_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(|p| p.join("comfy")))?;
    (dir.join("python_embeded").is_dir() && dir.join("ComfyUI").join("main.py").is_file()).then_some(dir)
}

/// What runs the models. AMD, Intel and no GPU all work on the CPU, only slowly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Compute {
    Gpu,
    Cpu,
}

impl Compute {
    /// Asked once per host run: a graphics card doesn't come and go while it runs.
    pub fn current() -> Self {
        static CURRENT: OnceLock<Compute> = OnceLock::new();
        *CURRENT.get_or_init(Self::detect)
    }

    fn detect() -> Self {
        let found = std::process::Command::new("nvidia-smi")
            .arg("-L")
            .stdin(std::process::Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("GPU"));
        if found { Self::Gpu } else { Self::Cpu }
    }

    pub fn notice(self) -> Option<&'static str> {
        match self {
            Self::Gpu => None,
            Self::Cpu => Some(
                "No NVIDIA graphics card was found, so ComfyUI runs on the processor. Images take minutes instead of seconds.",
            ),
        }
    }
}

/// The `extra_model_paths.yaml` that links the user's folders.
fn model_paths_yaml(base: &Path) -> String {
    let folders: String =
        MODEL_FOLDERS.iter().map(|(key, folder)| ["    ", key, ": ", folder, "\n"].concat()).collect();
    let base = base.to_string_lossy().replace('\'', "''");
    format!("codync:\n    base_path: '{base}'\n    is_default: true\n{folders}")
}

/// Creates the data folders and the yaml, and returns the yaml's path.
fn prepare_data(data: &Path) -> Result<PathBuf> {
    let mut folders = vec!["user", "input", "output", "temp"].into_iter().map(PathBuf::from).collect::<Vec<_>>();
    folders.extend(MODEL_FOLDERS.iter().map(|(_, f)| PathBuf::from(f)));
    for folder in folders {
        std::fs::create_dir_all(data.join(folder)).with_context(|| format!("creating {}", data.display()))?;
    }
    let yaml = data.join("extra_model_paths.yaml");
    std::fs::write(&yaml, model_paths_yaml(data)).context("writing extra_model_paths.yaml")?;
    Ok(yaml)
}

/// Free bytes on the disk that holds `path`.
fn free_space(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(sysinfo::Disk::available_space)
}

/// Checks before the first start, then the process to run on `port`.
pub fn launch(port: u16) -> Result<Launch> {
    let Some(program) = program_dir() else {
        bail!("ComfyUI is not bundled in this build (no comfy/ folder beside the host)");
    };
    let data = data_dir();
    std::fs::create_dir_all(&data).context("creating the ComfyUI data folder")?;
    if let Some(free) = free_space(&data) {
        ensure!(
            free >= MIN_FREE || data.join("user").is_dir(),
            "ComfyUI needs {} GB free on this disk to start for the first time; {} GB are free",
            MIN_FREE >> 30,
            free >> 30
        );
    }
    let yaml = prepare_data(&data)?;
    let arg = |s: &str| std::ffi::OsString::from(s);
    let mut args = vec![
        arg("-s"),
        program.join("ComfyUI").join("main.py").into_os_string(),
        arg("--listen"),
        arg("127.0.0.1"),
        arg("--port"),
        port.to_string().into(),
        arg("--disable-auto-launch"),
    ];
    for (flag, folder) in [
        ("--user-directory", "user"),
        ("--input-directory", "input"),
        ("--output-directory", "output"),
        ("--temp-directory", "temp"),
    ] {
        args.push(arg(flag));
        args.push(data.join(folder).into_os_string());
    }
    args.push(arg("--extra-model-paths-config"));
    args.push(yaml.into_os_string());
    if Compute::current() == Compute::Cpu {
        args.push(arg("--cpu"));
    }
    Ok(Launch {
        program: program.join("python_embeded").join(if cfg!(windows) { "python.exe" } else { "python3" }),
        args,
        cwd: program.join("ComfyUI"),
        env: vec![("PYTHONIOENCODING".into(), "utf-8".into()), ("PYTHONUNBUFFERED".into(), "1".into())],
    })
}

/// Deletes everything of the user's ComfyUI: models, custom nodes, images, settings. Only the uninstaller
/// asks for it, after the user said so; by default an uninstall keeps all of it.
pub fn remove_data() -> Result<()> {
    let dir = data_dir();
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("deleting {}", dir.display()))?;
    }
    Ok(())
}

/// How often the card's "space used" is measured again: walking a models folder is slow.
const USED_REFRESH: Duration = Duration::from_secs(30);

/// Bytes under `dir`, for the card's "space used". Links are not followed: a models folder linked in
/// from elsewhere is not this app's space, and a link loop must not recurse forever.
pub fn used_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_symlink() => 0,
            Ok(t) if t.is_dir() => used_bytes(&e.path()),
            Ok(_) => e.metadata().map_or(0, |m| m.len()),
            Err(_) => 0,
        })
        .sum()
}

/// [`used_bytes`] of the data folder, measured at most every [`USED_REFRESH`].
fn used_bytes_recent() -> u64 {
    static LAST: Mutex<Option<(Instant, u64)>> = Mutex::new(None);
    if let Some((at, bytes)) = *LAST.locked()
        && at.elapsed() < USED_REFRESH
    {
        return bytes;
    }
    let bytes = used_bytes(&data_dir());
    *LAST.locked() = Some((Instant::now(), bytes));
    bytes
}

/// What the card shows beside the runner's state. Blocking (it measures the data folder): call it
/// through `spawn_blocking`.
pub fn card() -> Value {
    json!({
        "version": pin().version,
        "bundled": program_dir().is_some(),
        "dataDir": data_dir().to_string_lossy(),
        "modelsDir": checkpoints_dir().to_string_lossy(),
        "modelsUrl": MODELS_URL,
        "usedBytes": used_bytes_recent(),
        "notice": Compute::current().notice(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_names_a_version_and_a_commit() {
        assert!(pin().version.starts_with('v'));
        assert_eq!(pin().commit.len(), 40);
    }

    #[test]
    fn the_yaml_links_every_folder_under_the_base() {
        let yaml = model_paths_yaml(Path::new("C:/Users/it's me/comfy"));
        assert!(yaml.starts_with("codync:\n    base_path: 'C:/Users/it''s me/comfy'\n"));
        assert!(yaml.contains("    checkpoints: models/checkpoints/\n"));
        assert!(yaml.contains("    custom_nodes: custom_nodes/\n"));
    }

    #[test]
    fn removing_the_data_deletes_the_comfy_folder_and_only_it() {
        let data = data_dir();
        prepare_data(&data).expect("prepared");
        let neighbour = data.parent().expect("a parent").join("keep-me.txt");
        std::fs::write(&neighbour, "x").expect("a neighbour file");
        remove_data().expect("removed");
        assert!(!data.exists() && neighbour.is_file());
        remove_data().expect("removing nothing is fine");
        let _ = std::fs::remove_file(neighbour);
    }

    #[test]
    fn the_space_used_counts_every_file_below() {
        let dir = std::env::temp_dir().join(format!("codync-comfy-used-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("models/checkpoints")).expect("folders");
        std::fs::write(dir.join("a.bin"), [0_u8; 10]).expect("a file");
        std::fs::write(dir.join("models/checkpoints/b.safetensors"), [0_u8; 32]).expect("a nested file");
        assert_eq!(used_bytes(&dir), 42);
        assert_eq!(used_bytes(&dir.join("missing")), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preparing_data_creates_the_folders_and_the_yaml() {
        let dir = std::env::temp_dir().join(format!("codync-comfy-{}", std::process::id()));
        let yaml = prepare_data(&dir).expect("prepared");
        assert!(dir.join("user").is_dir() && dir.join("models/checkpoints").is_dir());
        assert!(std::fs::read_to_string(yaml).expect("yaml").contains("is_default: true"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
