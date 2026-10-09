//! Shared by the end-to-end tests.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Use the checksum-verified release fixture so isolated hosts don't each download it.
pub fn seed_memory_runtime(home: &std::path::Path) {
    let Some(binary) = std::env::var_os("CODYNC_TEST_ENGRAM") else {
        return;
    };
    let dir = home.join("engram/3.2.1");
    std::fs::create_dir_all(&dir).expect("memory runtime folder");
    std::fs::copy(binary, dir.join(if cfg!(windows) { "engram.exe" } else { "engram" })).expect("copy Engram fixture");
    std::fs::write(dir.join(".installed"), "3.2.1").expect("mark fixture installed");
}

/// Stops a test host the way launchd does (SIGTERM), so it shuts down its own
/// children: a SIGKILL would orphan whatever it had running, such as a harness
/// sign-in check (`cursor-agent status` then spins forever). Kills it after 5 s.
pub fn stop(child: &mut Child) {
    let _ = Command::new("kill").arg(child.id().to_string()).status();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Python as the test machine names it.
pub fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

/// The agent command that runs a Python fixture (quoted for `sh -c` / `cmd /c`).
pub fn python_agent(script: &std::path::Path) -> String {
    let path = script.display().to_string();
    if cfg!(windows) {
        format!("{} -u \"{path}\"", python())
    } else {
        format!("{} -u '{}'", python(), path.replace('\'', "'\\''"))
    }
}
