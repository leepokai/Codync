//! When `serve` stops.

/// Resolves on Ctrl-C, SIGTERM, or, with `CODYNC_EXIT_WITH_STDIN=1`, once stdin closes. The tests'
/// disposable hosts set it and hold stdin open, so a test run that is killed (and never gets to stop
/// its hosts) still takes them down with it: the system closes the pipe when the run exits.
pub async fn signal() {
    tokio::select! {
        () = ctrl_c() => {}
        () = terminate() => {}
        () = stdin_closed() => {}
    }
}

async fn ctrl_c() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(%error, "can't listen for Ctrl-C");
        std::future::pending::<()>().await;
    }
}

#[cfg(unix)]
async fn terminate() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut s) => {
            s.recv().await;
        }
        Err(error) => {
            tracing::warn!(%error, "can't listen for SIGTERM");
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(not(unix))]
async fn terminate() {
    std::future::pending::<()>().await;
}

/// The service's stdin is `/dev/null`, closed from the start, so only a host told to watches it.
async fn stdin_closed() {
    if std::env::var("CODYNC_EXIT_WITH_STDIN").is_ok_and(|value| value == "1") {
        // Reads until end of file, then the host stops; a read error stops it too.
        let _ = tokio::io::copy(&mut tokio::io::stdin(), &mut tokio::io::sink()).await;
        tracing::info!("stdin closed");
    } else {
        std::future::pending::<()>().await;
    }
}
