use super::*;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("codync-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn snapshots_preserve_binary_empty_and_hidden_files_after_source_changes() {
    let temp = Temp::new();
    for (name, data) in
        [("report.bin", vec![0, 255, 1, 128]), (".hidden", vec![]), ("no-extension", vec![42; CHUNK_SIZE + 13])]
    {
        std::fs::write(temp.0.join(name), &data).unwrap();
        let root = temp.0.join("snapshots");
        let mut saved = snapshot(&root, &temp.0, name, None, &AtomicBool::new(false)).unwrap();
        saved.commit();
        std::fs::remove_file(temp.0.join(name)).unwrap();
        let mut downloaded = read(&root, &saved.meta, 0).unwrap();
        if downloaded.len() < data.len() {
            downloaded.extend(read(&root, &saved.meta, CHUNK_SIZE as u64).unwrap());
        }
        assert_eq!(downloaded, data);
        assert_eq!(saved.meta.name, name);
        assert_eq!(saved.meta.size, data.len() as u64);
        assert_eq!(read(&root, &saved.meta, data.len() as u64).unwrap(), Vec::<u8>::new());
        assert!(read(&root, &saved.meta, data.len() as u64 + 1).is_err());
    }
}

#[test]
fn failures_and_unpublished_snapshots_leave_no_files() {
    let temp = Temp::new();
    let root = temp.0.join("snapshots");
    std::fs::write(temp.0.join("file"), [1]).unwrap();
    assert!(snapshot(&root, &temp.0, "missing", None, &AtomicBool::new(false)).is_err());
    assert!(snapshot(&root, &temp.0, ".", None, &AtomicBool::new(false)).is_err());
    assert!(snapshot(&root, &temp.0, "file", Some("../escape"), &AtomicBool::new(false)).is_err());
    assert!(snapshot(&root, &temp.0, "file", None, &AtomicBool::new(true)).is_err());
    let saved = snapshot(&root, &temp.0, "file", None, &AtomicBool::new(false)).unwrap();
    drop(saved);
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn rejects_large_files_and_damaged_snapshots() {
    let temp = Temp::new();
    let root = temp.0.join("snapshots");
    let file = File::create(temp.0.join("large")).unwrap();
    file.set_len(MAX_FILE).unwrap();
    let exact = snapshot(&root, &temp.0, "large", None, &AtomicBool::new(false)).unwrap();
    assert_eq!(exact.meta.size, MAX_FILE);
    drop(exact);
    file.set_len(MAX_FILE + 1).unwrap();
    assert!(snapshot(&root, &temp.0, "large", None, &AtomicBool::new(false)).is_err());
    std::fs::write(temp.0.join("file"), [42]).unwrap();
    let saved = snapshot(&root, &temp.0, "file", None, &AtomicBool::new(false)).unwrap();
    std::fs::write(&saved.path, []).unwrap();
    assert!(read(&root, &saved.meta, 0).is_err());
    assert!(path(&root, "../file").is_err());
}

#[cfg(unix)]
#[test]
fn snapshot_reads_symlink_sources_but_never_symlink_storage_or_special_files() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    let root = temp.0.join("snapshots");
    std::fs::write(temp.0.join("file"), [42]).unwrap();
    symlink(temp.0.join("file"), temp.0.join("link")).unwrap();
    let saved = snapshot(&root, &temp.0, "link", Some("linked.dat"), &AtomicBool::new(false)).unwrap();
    std::fs::remove_file(&saved.path).unwrap();
    symlink(temp.0.join("file"), &saved.path).unwrap();
    assert!(read(&root, &saved.meta, 0).is_err());
    assert!(snapshot(&root, &temp.0, "/dev/null", Some("device"), &AtomicBool::new(false)).is_err());
}

#[test]
fn cancellation_during_copy_stops_before_reading_the_next_chunk() {
    struct CancellingReader<'a>(&'a AtomicBool, usize);
    impl std::io::Read for CancellingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.1 += 1;
            self.0.store(true, Ordering::Release);
            buffer[0] = 42;
            Ok(1)
        }
    }
    let cancelled = AtomicBool::new(false);
    let mut reader = CancellingReader(&cancelled, 0);
    let mut output = Vec::new();
    assert!(copy(&mut reader, &mut output, &cancelled).is_err());
    assert_eq!(reader.1, 1);
    assert_eq!(output, [42]);
}

#[test]
fn names_never_carry_path_or_drive_syntax() {
    for bad in ["", ".", "..", "a/b", "a\\b", "C:evil.bat", "line\nbreak"] {
        assert!(name(bad).is_err(), "{bad:?}");
    }
    assert_eq!(name(".hidden").unwrap(), ".hidden");
}
