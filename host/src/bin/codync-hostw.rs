//! Windows' service runner: starts `codync-host.exe` beside it with the given
//! arguments and no console window, and starts it again when it exits (what
//! launchd's `KeepAlive` and systemd's `Restart=always` do elsewhere). It is a
//! GUI-subsystem program, so signing in doesn't flash a console either.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
#[path = "../environment.rs"]
#[allow(dead_code)]
mod environment;

#[cfg(windows)]
fn main() {
    use std::os::windows::process::CommandExt as _;
    use std::time::Duration;
    /// The host gets a hidden console, which the agents it starts share.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Ok(exe) = std::env::current_exe() else { return };
    let host = exe.with_file_name("codync-host.exe");
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let data = std::env::var_os("CODYNC_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(environment::Environment::current().data_folder())));
    loop {
        let mut command = std::process::Command::new(&host);
        command.args(&args).creation_flags(CREATE_NO_WINDOW);
        let log = data.as_ref().and_then(|d| {
            std::fs::create_dir_all(d).ok()?;
            std::fs::OpenOptions::new().create(true).append(true).open(d.join("host.log")).ok()
        });
        if let Some(log) = log
            && let Ok(err) = log.try_clone()
        {
            command.stdout(log).stderr(err);
        }
        // Whatever the exit (a crash, a host that can't start yet), try again shortly.
        let _ = command.status();
        std::thread::sleep(Duration::from_secs(3));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("codync-hostw only runs the host on Windows; use codync-host here.");
    std::process::exit(2);
}
