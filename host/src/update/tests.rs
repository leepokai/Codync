use super::{install, release};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signer as _, SigningKey};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("codync-update-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn manifest() -> release::Release {
    release::Release {
        version: "3.0.0".into(),
        platform: "linux-arm64".into(),
        url: format!("{}/download/v3.0.0/codync-host-linux-arm64.tar.gz", release::RELEASES),
        sha256: "a".repeat(64),
        size: 12,
    }
}

#[test]
fn only_signed_platform_matched_manifests_are_accepted() {
    let key = SigningKey::from_bytes(&[17; 32]);
    let public = B64.encode(key.verifying_key().as_bytes());
    let bytes = serde_json::to_vec(&manifest()).unwrap();
    let signature = B64.encode(key.sign(&bytes).to_bytes());
    assert_eq!(release::verify_manifest(&bytes, &signature, &public, "linux-arm64").unwrap().version, "3.0.0");
    assert!(release::verify_manifest(&bytes, &signature, &public, "macos-arm64").is_err());
    let mut altered = bytes.clone();
    altered[0] ^= 1;
    assert!(release::verify_manifest(&altered, &signature, &public, "linux-arm64").is_err());
    let other = B64.encode(SigningKey::from_bytes(&[18; 32]).verifying_key().as_bytes());
    assert!(release::verify_manifest(&bytes, &signature, &other, "linux-arm64").is_err());
    let mut redirected = manifest();
    redirected.url = "https://example.com/host.tar.gz".into();
    let redirected = serde_json::to_vec(&redirected).unwrap();
    let signature = B64.encode(key.sign(&redirected).to_bytes());
    assert!(release::verify_manifest(&redirected, &signature, &public, "linux-arm64").is_err());
}

#[test]
fn only_signed_compat_files_for_the_release_are_accepted() {
    let key = SigningKey::from_bytes(&[17; 32]);
    let public = B64.encode(key.verifying_key().as_bytes());
    let sign = |bytes: &[u8]| B64.encode(key.sign(bytes).to_bytes());
    let compat = br#"{"version": "3.0.0", "minApp": "2.9.0"}"#;
    assert_eq!(release::verify_compat(compat, &sign(compat), &public, "3.0.0").unwrap(), "2.9.0");
    // Another release's file, a forged one, or one without a readable minApp is refused.
    assert!(release::verify_compat(compat, &sign(compat), &public, "3.0.1").is_err());
    let other = B64.encode(SigningKey::from_bytes(&[18; 32]).verifying_key().as_bytes());
    assert!(release::verify_compat(compat, &sign(compat), &other, "3.0.0").is_err());
    let mut altered = compat.to_vec();
    altered[30] = b'8';
    assert!(release::verify_compat(&altered, &sign(compat), &public, "3.0.0").is_err());
    let bad = br#"{"version": "3.0.0", "minApp": "soon"}"#;
    assert!(release::verify_compat(bad, &sign(bad), &public, "3.0.0").is_err());
    // A signed release manifest isn't a compat file.
    let manifest = serde_json::to_vec(&manifest()).unwrap();
    assert!(release::verify_compat(&manifest, &sign(&manifest), &public, "3.0.0").is_err());
}

fn archive(symlink: bool) -> Vec<u8> {
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gzip);
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o755);
    header.set_size(if symlink { 0 } else { 3 });
    if symlink {
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("/tmp/unrelated-executable").unwrap();
    }
    header.set_cksum();
    let contents: &[u8] = if symlink { b"" } else { b"new" };
    tar.append_data(&mut header, "codync-host-linux-arm64/codync-host", contents).unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

#[test]
fn extraction_refuses_symlinks_and_only_creates_the_expected_binary() {
    let dir = Directory::new();
    let target = dir.0.join("candidate");
    assert!(release::extract(&archive(true), "linux-arm64", &target).is_err());
    assert!(!target.exists());
    assert!(release::extract(&archive(false), "macos-arm64", &target).is_err());
    release::extract(&archive(false), "linux-arm64", &target).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert!(release::extract(&archive(false), "linux-arm64", &target).is_err());
}

#[test]
fn the_screen_helper_is_extracted_only_when_the_archive_carries_it() {
    let dir = Directory::new();
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gzip);
    for (name, contents) in [("codync-host", b"new"), ("codync-screen", b"scr")] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_size(3);
        header.set_cksum();
        tar.append_data(&mut header, format!("codync-host-linux-arm64/{name}"), &contents[..]).unwrap();
    }
    let with_helper = tar.into_inner().unwrap().finish().unwrap();
    let helper = dir.0.join("helper");
    assert!(release::extract_file(&with_helper, "linux-arm64", "codync-screen", &helper).unwrap());
    assert_eq!(std::fs::read(&helper).unwrap(), b"scr");
    assert!(!release::extract_file(&archive(false), "linux-arm64", "codync-screen", &dir.0.join("none")).unwrap());
}

#[tokio::test]
async fn a_failed_health_check_restores_the_previous_binary_and_service() {
    let dir = Directory::new();
    let target = dir.0.join("host");
    let candidate = dir.0.join("candidate");
    std::fs::write(&target, b"old").unwrap();
    std::fs::write(&candidate, b"new").unwrap();
    let restored = AtomicBool::new(false);
    let result = install::replace(
        &target,
        &candidate,
        || async { Ok(()) },
        || async {
            assert_eq!(std::fs::read(&target).unwrap(), b"new");
            anyhow::bail!("new host never answered")
        },
        || async {
            assert_eq!(std::fs::read(&target).unwrap(), b"old");
            restored.store(true, Ordering::SeqCst);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err());
    assert!(restored.load(Ordering::SeqCst));
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[tokio::test]
async fn failure_to_stop_the_old_host_never_replaces_its_executable() {
    let dir = Directory::new();
    let target = dir.0.join("host");
    let candidate = dir.0.join("candidate");
    std::fs::write(&target, b"old").unwrap();
    std::fs::write(&candidate, b"new").unwrap();
    let result = install::replace(
        &target,
        &candidate,
        || async { anyhow::bail!("host is still running") },
        || async { panic!("must not start the new host") },
        || async { panic!("old binary was never replaced") },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"old");
}

#[tokio::test]
async fn a_healthy_update_replaces_the_binary_and_removes_its_backup() {
    let dir = Directory::new();
    let target = dir.0.join("host");
    let candidate = dir.0.join("candidate");
    std::fs::write(&target, b"old").unwrap();
    std::fs::write(&candidate, b"new").unwrap();
    install::replace(
        &target,
        &candidate,
        || async { Ok(()) },
        || async { Ok(()) },
        || async { panic!("must not roll back a healthy update") },
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(target).unwrap(), b"new");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn app_and_homebrew_binaries_keep_their_installation_owner() {
    assert_eq!(
        install::method(std::path::Path::new("/Applications/Codync.app/Contents/Resources/codync-host")),
        install::Method::AppBundle
    );
    assert_eq!(
        install::method(std::path::Path::new("/opt/homebrew/Cellar/codync-host/2.2.1/bin/codync-host")),
        install::Method::Homebrew
    );
}
