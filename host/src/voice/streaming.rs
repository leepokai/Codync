//! Half-second PCM frames from native `NeuTTS` inference, wrapped as independently playable WAVs.
use crate::agent::{hermes::Installation, process::Process};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
};

async fn packet(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Option<Value>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            ensure!(line.is_empty(), "Speech frame ended prematurely");
            return Ok(None);
        }
        let length = buffer.iter().position(|b| *b == b'\n').map_or(buffer.len(), |i| i + 1);
        ensure!(line.len() + length <= 48 * 1024, "Speech frame exceeds its limit");
        line.extend_from_slice(&buffer[..length]);
        reader.consume(length);
        if line.last() == Some(&b'\n') {
            return Ok(Some(serde_json::from_slice(&line)?));
        }
    }
}

pub(crate) async fn produce(installation: &Installation, text: &str, output: mpsc::Sender<Value>) -> Result<()> {
    ensure!(!text.trim().is_empty() && text.len() <= 4000 && !text.contains('\0'), "Invalid speech text");
    let work = async {
        let _permit = super::neutts::SYNTHESIS.acquire().await?;
        ensure!(super::neutts::ready(installation)?, "Prepare NeuTTS with hermes setup tts before using local speech");
        let mut command = tokio::process::Command::new(&installation.python);
        command
            .current_dir(&installation.root)
            .args(["-I", "-u", "-c", include_str!("neutts_stream.py")])
            .arg(&installation.root)
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("HF_DATASETS_OFFLINE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        command.creation_flags(
            windows_sys::Win32::System::Threading::CREATE_SUSPENDED
                | windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
        );
        let mut child = command.spawn().context("Could not start local streaming speech")?;
        let stdout = child.stdout.take().context("Speech stdout is unavailable")?;
        let mut stderr = child.stderr.take().context("Speech stderr is unavailable")?;
        let mut input = child.stdin.take().context("Speech stdin is unavailable")?;
        let mut owned = Process::new(child)?;
        input.write_all(&serde_json::to_vec(&json!({"text":text}))?).await?;
        input.shutdown().await?;
        drop(input);
        let read = async {
            let mut reader = BufReader::new(stdout);
            let (mut total, mut count) = (0_usize, 0_usize);
            loop {
                let value = packet(&mut reader).await?.context("Speech stream ended without completion")?;
                if value["done"] == true {
                    ensure!(count > 0, "NeuTTS produced no speech");
                    break;
                }
                let pcm = STANDARD.decode(value["pcm"].as_str().context("Invalid speech frame")?)?;
                ensure!(!pcm.is_empty() && pcm.len() <= 24_000 && pcm.len() % 2 == 0, "Invalid PCM frame");
                total += pcm.len();
                count += 1;
                ensure!(total <= 48_000 * 80, "Speech stream is too long");
                output
                    .send(json!({"audio":STANDARD.encode(super::wav(&pcm,24_000)),"format":"wav","done":false}))
                    .await
                    .context("Speech listener closed")?;
            }
            anyhow::Ok(())
        };
        let errors = async {
            let mut bytes = [0_u8; 4096];
            let mut count = 0;
            loop {
                let n = stderr.read(&mut bytes).await?;
                if n == 0 {
                    break;
                }
                count += n;
                ensure!(count <= 128 * 1024, "Speech diagnostics exceeded their limit");
            }
            anyhow::Ok(())
        };
        let ((), (), status) = tokio::try_join!(read, errors, owned.wait_owned())?;
        ensure!(status.success(), "NeuTTS streaming failed; check its prepared native environment");
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(80), work).await.context("Streaming speech timed out; worker stopped")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_rejects_truncation_and_oversized_packets() {
        let mut valid = BufReader::new(&b"{\"done\":true}\n"[..]);
        assert_eq!(packet(&mut valid).await.unwrap().unwrap()["done"], true);
        assert!(packet(&mut valid).await.unwrap().is_none());
        let mut partial = BufReader::new(&b"{\"done\":true}"[..]);
        assert!(packet(&mut partial).await.is_err());
        let bytes = vec![b'x'; 48 * 1024 + 1];
        assert!(packet(&mut BufReader::new(bytes.as_slice())).await.is_err());
    }
}
