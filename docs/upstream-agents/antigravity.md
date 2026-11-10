# Antigravity

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Signed out, it starts its browser sign-in as soon as a session opens, so a usage reading would open a browser at every poll | `host/src/usage/acp_read.rs`: started with `BROWSER=true`, which its Python honors. Windows has no such command, so `host/src/usage/antigravity.rs` needs a saved login file first | No issue opened yet: this only surfaced reading usage | `session/new` doesn't start a sign-in |
| Each new session leaves a conversation database behind, about 400 KB | `host/src/usage/acp_read.rs`: readings share one session, resumed each time (`usage.session.<id>` in the kv) | No issue opened yet: keeping sessions may be intended | A session with no turn leaves nothing behind |
| On Windows its server (`agy_acp_server.exe`, a `PyInstaller` onefile) ignores its closed stdin (still running 15 s later), and ended together with its launcher it leaves the launcher's ~131 MB `_MEI*` folder in `%TEMP%`. Every stop did, about every five minutes with the usage reading ([#399](https://github.com/leepokai/Codync/issues/399)) | `host/src/agent/process.rs`: once the grace period is over, a stop ends the tree from its leaves up and gives a process whose children have ended 3 s to finish (`PARENT_GRACE`), so the launcher removes its folder. Not covered: a forced stop still leaves it (the Windows service stop's `taskkill /F /T`, which app updates and **Restart host** run, signing out, a host crash), and nothing removes folders left before; the manual says how (`web/content/docs/agents.mdx`) | No issue opened yet: needs a report from someone who can reproduce it on Windows | It exits on its closed stdin. The unwinding stays: it is how every tree stops |
