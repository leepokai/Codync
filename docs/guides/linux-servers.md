# Running the host on a Linux server or cloud VM

`codync-host` runs on any 64-bit Linux machine: a desktop, a home server, or a headless cloud VM (AWS, GCP, Azure, Hetzner, DigitalOcean…). It needs no display. Bots run on that machine and you message them from the iPhone, the desktop app or `codync-host tui`.

## Requirements

| Need | Why |
| --- | --- |
| x86_64 or arm64 Linux | Release builds exist for both architectures |
| `ca-certificates` | HTTPS to the ACP registry, push relay and cloud. Every normal VM image has it; bare container images (`debian`, `ubuntu`) need `apt-get install ca-certificates`. Without it every command exits with `TLS setup failed: install the ca-certificates package` |
| systemd | Only for `codync-host install` (background service). Without systemd, run `codync-host serve` under your own supervisor |
| Node.js (optional) | Claude Code, Codex and Pi run through ACP adapters fetched with `npx`. Without Node, an agent's setup offers to add Codync's own copy (glibc distros; over SSH: `codync-host node`); on musl distros (Alpine) install Node.js |
| The agents you want, signed in | The host drives the CLIs installed on this machine with their own credentials. Sign in over SSH (`claude`, `codex login`…) as the same user that runs the host |

Distro and C library don't matter. From the first release built by the updated workflow, Linux release binaries are static musl builds with no glibc dependency. Earlier releases (≤ 2.1.0) were built against glibc 2.39 and only start on Ubuntu 24.04 or newer. On older distros they fail with `GLIBC_2.39 not found`; build from source or use a newer release there.

## Install

Run as the user who owns the agents (not root, unless the agents are root's):

```sh
curl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh -s -- --host-only
# or: brew install leepokai/codync/codync-host
codync-host install                    # systemd --user service, restarts on crash
sudo loginctl enable-linger "$USER"    # keep it running after you log out of SSH
codync-host status
```

The script puts `codync-host` in `~/.local/bin` (`/usr/local/bin` as root; override with `CODYNC_BIN_DIR`), checks its SHA-256, and restarts the service on upgrade. `install` saves the current `PATH` into the service, so install Node and the agents first. If you add an agent later and the host doesn't find it, run `codync-host install` again. `install` needs a real login session (SSH login, not `sudo su user`), otherwise `systemctl --user` can't reach the user's service manager.

Other commands work the same as on a desktop: `codync-host pair`, `codync-host tui`, `codync-host uninstall` (keeps data in `~/.codync`). Logs go to the journal: `journalctl --user -u codync-host -f`.

## Reaching it from the phone

The phone connects over the encrypted device channel. The pairing link lists only **Tailscale** and **private (LAN/VPC)** addresses, plus any you set in `CODYNC_EXTERNAL_URL`; the host never advertises a public IP on its own.

- **Tailscale (recommended for VMs):** install Tailscale on the VM and on the phone, then `codync-host pair`. The Tailscale name comes first in the link. You don't need to open port 19222 in the cloud firewall.
- **Codync relay (on by default):** a release host connects to the production cloud (`https://api.codync.dev`), so a paired phone reaches it from anywhere with nothing to open. `codync-host cloud --disable` turns it off; `CODYNC_CLOUD_URL=https://…` points the host at a cloud you run yourself (see [environments](environments-and-deployment.md)).
- **Your own HTTPS reverse proxy:** forward only `/channel` (WebSocket upgrade) at the root of a hostname to the host; don't forward `/api`, `/events`, `/term` or `/oauth`: through a local proxy every request looks like it comes from this computer, so the loopback-only checks no longer protect them. Then set `CODYNC_EXTERNAL_URL=https://codync.example.com` (comma-separated for several) in the host's environment: `systemctl --user edit codync-host`, add `[Service]` and `Environment=CODYNC_EXTERNAL_URL=https://codync.example.com`, then `systemctl --user restart codync-host`. The host advertises those URLs first, in the pairing link and on every connection, so the phone keeps them (a phone keeps only the addresses the host advertises). Under your own supervisor, `CODYNC_EXTERNAL_URL=… codync-host serve --bind 127.0.0.1` keeps the host on loopback behind the proxy. The phone dials `<url>/channel?v=1`. Opening `/channel` in a browser or with `curl` gets `400` (no WebSocket upgrade), and an upgrade without `?v=1` gets `426` `upgradeRequired`: that means the proxy works. A working proxy answers the phone's upgrade with `101`.
- **Desktop app over SSH:** the desktop app (macOS or Linux) can attach an SSH computer and tunnel to its loopback API ([Accounts and SSH](accounts-and-ssh.md)). From any terminal: `ssh -L 19222:127.0.0.1:19222 vm`, then `codync-host tui` locally.

The host listens on `0.0.0.0:19222`. The local API (`/api/*`, `/events`, terminals; bearer token) answers **loopback callers only**, so a public address never exposes it. From outside, only `/health` (host id, computer id, version, environment; the binary path and hash and `busy` go to loopback callers only), local routine webhooks (`/hooks/routines/{id}`, which need the routine's key) and the end-to-end encrypted device channel answer, and the channel only serves devices holding this host's pairing keys. Keep 19222 closed in the cloud firewall anyway unless you deliberately pair over a VPN or private network.

## Limitations on a headless server

- **Remote screen and the `computer` tool don't work.** They need a desktop session (portals + GStreamer, `apps/screen-linux/`). The feature turns itself on only while a graphical session is running, so on a server it stays off and nothing else is affected.
- **No desktop app.** The Linux desktop app (`codync`, AppImage/deb/tar.gz) needs a graphical session; it uses the installed `codync-host`. On a server, use the phone, the desktop app on another computer over SSH, or `codync-host tui`.
- **Keep-awake is a no-op.** The host tries `systemd-inhibit` while a bot works; VMs don't sleep, and failure only logs a warning.
- **One computer per account.** A VM counts as the account's computer; multiple computers are future work.
- **Usage limits** come from the local Claude and Codex installs, same as on a desktop.

## Verification — 2026-09-26

Tested in Docker on arm64 (the x86_64 build is the same code and the same CI job):

- Static musl build of `host/` 2.2.0 started on Ubuntu 22.04, Debian 12, Amazon Linux 2023, Rocky Linux 9 and Alpine 3. `serve`, `status`, `pair --json`, `info` and `cloud` worked with no display.
- On a systemd container (Ubuntu 22.04, lingering user): `install` created and started the service, `pair` worked, a killed host was restarted by systemd with a new PID, and `uninstall` removed it and kept the data.
- The v2.1.0 glibc release failed on Ubuntu 22.04 and Debian 12 (`GLIBC_2.39 not found`). The workflow now builds musl for that reason.
- Host tests passed on Linux: 101 + 9 + 1.

Not covered: a real cloud provider VM, pairing a physical iPhone over Tailscale to a VM, and bot turns with signed-in agents on the VM (these need real credentials). Those use the same code paths as a Linux desktop.
