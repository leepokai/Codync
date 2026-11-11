//! Source: Hermes tools/neutts_synth.py and tools/neutts_samples at 10d25f8a (MIT).
//! Adaptation: reuse its installed Python, own its process tree, and return bounded WAV clips.

use crate::agent::hermes::Installation;
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt as _;

// Base64 plus the RPC envelope must fit the paired channel's 1 MiB message limit.
const MAX_AUDIO_BYTES: usize = 700 * 1024;
const MAX_TEXT_BYTES: usize = 4_000;
// Clients already budget ninety seconds for a clip; leave time to return a useful error.
const DEADLINE: Duration = Duration::from_secs(80);
const SETUP: &str = "Run `hermes setup tts`, choose NeuTTS, and prepare its models and espeak-ng before using local speech (NeuTTS requires Python 3.11–3.13).";
pub(super) static SYNTHESIS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

fn is_file(path: &Path) -> Result<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("Could not inspect the local NeuTTS installation"),
    }
}

fn ready_paths(python: &Path, root: &Path, espeak: bool) -> Result<bool> {
    if !espeak {
        return Ok(false);
    }
    for path in [
        python.to_owned(),
        root.join("tools/neutts_synth.py"),
        root.join("tools/neutts_samples/jo.wav"),
        root.join("tools/neutts_samples/jo.txt"),
    ] {
        if !is_file(&path)? {
            return Ok(false);
        }
    }
    let Some(environment) = python.parent().and_then(Path::parent) else { return Ok(false) };
    let mut sites = vec![environment.join("Lib/site-packages")];
    for minor in 11..=13 {
        for lib in ["lib", "lib64"] {
            sites.push(environment.join(format!("{lib}/python3.{minor}/site-packages")));
        }
    }
    for site in sites {
        if is_file(&site.join("neutts/__init__.py"))?
            && is_file(&site.join("numpy/__init__.py"))?
            && is_file(&site.join("llama_cpp/__init__.py"))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Static prerequisites only: no Python imports, inference or dependency downloads.
/// Model-cache or native-library failures are reported by synthesis, never hidden by a cloud fallback.
pub(crate) fn ready(installation: &Installation) -> Result<bool> {
    ready_paths(&installation.python, &installation.root, crate::agent::backends::which("espeak-ng").is_some())
}

struct TemporaryAudio(PathBuf);

impl TemporaryAudio {
    fn create(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory).context("Could not create the local speech output directory")?;
        let path = directory.join(format!("{}.wav", uuid::Uuid::new_v4()));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context("Could not reserve a local speech output file")?;
        Ok(Self(path))
    }
}

impl Drop for TemporaryAudio {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            _ => {}
        }
        // Windows may briefly retain the file while the cancelled process tree exits.
        // Retry only this request's reserved file; never recursively remove a directory.
        let path = self.0.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                for _ in 0..10 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    match tokio::fs::remove_file(&path).await {
                        Ok(()) => return,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                        _ => {}
                    }
                }
                tracing::warn!("Could not remove a temporary local speech file after process cancellation");
            });
        } else {
            tracing::warn!("Could not remove a temporary local speech file");
        }
    }
}

fn checked_wav(bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() > 44 && bytes.len() <= MAX_AUDIO_BYTES, "NeuTTS audio exceeds its size limit or is empty");
    ensure!(&bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE", "NeuTTS returned an invalid WAV container");
    let declared = u32::from_le_bytes(bytes[4..8].try_into()?) as usize;
    ensure!(declared.checked_add(8) == Some(bytes.len()), "NeuTTS returned a truncated WAV container");
    let (mut offset, mut format, mut data) = (12_usize, false, false);
    while offset + 8 <= bytes.len() {
        let tag = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        let start = offset + 8;
        let end = start.checked_add(size).context("NeuTTS WAV chunk is too large")?;
        ensure!(end <= bytes.len(), "NeuTTS returned a truncated WAV chunk");
        if tag == b"fmt " {
            ensure!(size >= 16, "NeuTTS returned an invalid WAV format");
            format = bytes[start..start + 4] == [1, 0, 1, 0]
                && u32::from_le_bytes(bytes[start + 4..start + 8].try_into()?) == 24_000
                && bytes[start + 14..start + 16] == [16, 0];
        } else if tag == b"data" {
            data = size > 0 && size % 2 == 0;
        }
        offset = end.checked_add(size % 2).context("NeuTTS WAV chunk is too large")?;
    }
    ensure!(offset == bytes.len() && format && data, "NeuTTS returned unsupported or incomplete WAV audio");
    Ok(())
}

/// A cache-only local synthesis; callers cancelling this future stop the complete worker tree.
pub(crate) async fn speak(installation: &Installation, text: &str) -> Result<Value> {
    ensure!(
        !text.trim().is_empty() && text.len() <= MAX_TEXT_BYTES && !text.contains('\0'),
        "text must be 1–4000 bytes without NUL characters"
    );
    let (python, root, text) = (installation.python.clone(), installation.root.clone(), text.to_owned());
    let work = async move {
        let _permit = SYNTHESIS.acquire().await.context("Local speech is unavailable")?;
        let (check_python, check_root) = (python.clone(), root.clone());
        let prepared = tokio::task::spawn_blocking(move || {
            ready_paths(&check_python, &check_root, crate::agent::backends::which("espeak-ng").is_some())
        })
        .await??;
        ensure!(prepared, "NeuTTS is not prepared. {SETUP}");
        let output =
            tokio::task::spawn_blocking(|| TemporaryAudio::create(&crate::service::data_dir().join("voice/neutts")))
                .await??;
        let mut command = tokio::process::Command::new(python);
        command
            .current_dir(&root)
            .arg("-I")
            .arg(root.join("tools/neutts_synth.py"))
            .arg(format!("--text={text}"))
            .arg("--out")
            .arg(&output.0)
            .arg("--ref-audio")
            .arg(root.join("tools/neutts_samples/jo.wav"))
            .arg("--ref-text")
            .arg(root.join("tools/neutts_samples/jo.txt"))
            .args(["--model", "neuphonic/neutts-air-q4-gguf", "--device", "cpu"])
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("HF_DATASETS_OFFLINE", "1");
        let outcome = crate::agent::process::output::run(command, 64 * 1024)
            .await
            .with_context(|| format!("NeuTTS could not synthesize speech. {SETUP}"))?;
        ensure!(outcome["success"] == true, "NeuTTS synthesis failed (exit code {}). {SETUP}", outcome["exitCode"]);
        let file = tokio::fs::File::open(&output.0).await.context("NeuTTS produced no speech file")?;
        let mut bytes = Vec::new();
        file.take(MAX_AUDIO_BYTES as u64 + 1).read_to_end(&mut bytes).await.context("Could not read NeuTTS speech")?;
        checked_wav(&bytes)?;
        Ok(json!({"audio":STANDARD.encode(&bytes), "format":"wav"}))
    };
    tokio::time::timeout(DEADLINE, work).await.context("Local speech timed out; its worker was stopped")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_validation_requires_complete_mono_pcm_at_the_helpers_sample_rate() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&38_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 1, 0]);
        wav.extend_from_slice(&24_000_u32.to_le_bytes());
        wav.extend_from_slice(&48_000_u32.to_le_bytes());
        wav.extend_from_slice(&[2, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&2_u32.to_le_bytes());
        wav.extend_from_slice(&[0, 0]);
        checked_wav(&wav).unwrap();
        assert!(checked_wav(&wav[..wav.len() - 1]).is_err());
        let mut wrong_rate = wav.clone();
        wrong_rate[24..28].copy_from_slice(&44_100_u32.to_le_bytes());
        assert!(checked_wav(&wrong_rate).is_err());
        let mut empty = wav.clone();
        empty[40..44].copy_from_slice(&0_u32.to_le_bytes());
        assert!(checked_wav(&empty).is_err());
        assert!(checked_wav(b"not audio").is_err());
    }

    #[test]
    fn temporary_audio_is_unique_and_deleted_when_ownership_ends() {
        let directory = std::env::temp_dir().join(format!("codync-neutts-{}", uuid::Uuid::new_v4()));
        let first = TemporaryAudio::create(&directory).unwrap();
        let second = TemporaryAudio::create(&directory).unwrap();
        let (a, b) = (first.0.clone(), second.0.clone());
        assert_ne!(a, b);
        drop(first);
        assert!(!a.exists());
        assert!(b.exists());
        drop(second);
        assert!(!b.exists());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn readiness_without_espeak_does_not_claim_a_working_installation() {
        assert!(!ready_paths(Path::new("missing-python"), Path::new("missing-hermes"), false).unwrap());
    }

    #[test]
    fn readiness_requires_the_existing_environments_dependencies_and_reference_files() {
        let directory = std::env::temp_dir().join(format!("codync-neutts-ready-{}", uuid::Uuid::new_v4()));
        let environment = directory.join(".venv");
        let python = environment.join("Scripts/python.exe");
        let site = environment.join("Lib/site-packages");
        let write = |path: &Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, []).unwrap();
        };
        for path in [
            python.clone(),
            directory.join("tools/neutts_synth.py"),
            directory.join("tools/neutts_samples/jo.wav"),
            directory.join("tools/neutts_samples/jo.txt"),
            site.join("neutts/__init__.py"),
            site.join("numpy/__init__.py"),
        ] {
            write(&path);
        }
        assert!(!ready_paths(&python, &directory, true).unwrap());
        write(&site.join("llama_cpp/__init__.py"));
        assert!(ready_paths(&python, &directory, true).unwrap());
        assert!(!ready_paths(&python, &directory, false).unwrap());
        std::fs::remove_file(directory.join("tools/neutts_samples/jo.wav")).unwrap();
        assert!(!ready_paths(&python, &directory, true).unwrap());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
