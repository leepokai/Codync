//! A real host whose runtime stops answering (a debug build told to act hung): the watchdog aborts it and leaves its
//! mark; the next start reports it; and a second stall soon after only logs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::eventually;
use common::host::Host;
use std::ffi::OsStr;
use std::time::Duration;

#[tokio::test]
async fn a_hung_host_is_aborted_once_and_a_quick_second_hang_only_logs() {
    let env = [("CODYNC_WATCHDOG_STALL_SECS", OsStr::new("2")), ("CODYNC_WATCHDOG_TEST_HANG", OsStr::new("1"))];
    let mut host = Host::start_with(&env).await;
    eventually("the watchdog aborted the host", Duration::from_secs(20), async || host.exited().is_some()).await;
    assert!(!host.exited().unwrap().success(), "aborted, not a clean exit");
    assert!(host.home.join("watchdog.restart").exists(), "it left its mark");

    // The service starts it again, still hung: the mark becomes the last restart, and it stays up.
    host.restart(|_| {}).await;
    assert!(!host.home.join("watchdog.restart").exists(), "the mark was reported");
    assert!(host.home.join("watchdog.last").exists());
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(host.exited().is_none(), "a second stall within half an hour only logs");
}
