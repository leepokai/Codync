//! An agent started after the host began stopping would outlive it: `process::exit` drops no process.
//! Its own test binary, since `stop_all` closes the process-wide list for good.
#![cfg(unix)]

#[tokio::test]
async fn an_agent_started_after_stop_all_is_refused() {
    codync_agent::acp::stop_all().await;
    let started = codync_agent::acp::Acp::spawn("sleep 300", "/tmp", &[]);
    let Err(err) = started else {
        let (acp, _rx) = started.expect("matched Ok");
        let pid = acp.pid();
        // Stands in for `process::exit`, which runs no drops.
        std::mem::forget(acp);
        if let Some(pid) = pid {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
        }
        panic!("an agent started after stop_all, nothing would stop it");
    };
    assert!(format!("{err:#}").contains("the host is stopping"), "{err:#}");
}
