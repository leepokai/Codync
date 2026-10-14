# Windows testing

Codync ships an x64 Windows app (Electron + the bundled `codync-host.exe`). Real Windows behavior is
checked in a UTM virtual machine on the Mac. Everyday end-to-end checks stay on the Mac; use the VM
when a change or a bug depends on Windows itself (paths, `cmd.exe`, `.cmd` shims, the service).

## The VM

- **UTM "Codync Windows"**: Windows 11 ARM64 (Codync's x64 build runs through Prism emulation),
  8 GB / 4 cores / 64 GB NVMe, account `codync` / `codync`, signs in automatically.
- **Start / stop** from the Mac, never through UTM's window:
  `osascript -e 'tell application "UTM" to start virtual machine named "Codync Windows"'`
  (`stop … by force` only when a normal stop hangs; `query ip of …` works). `utmctl` fails with
  OSStatus -10004. Pressing UTM's Run button through accessibility crashed UTM once; if UTM is in a
  bad state, quit it and reopen it in the background with `open -g -a UTM`.
- **Shell**: `ssh codync-win` (PowerShell). The `~/.ssh/config` alias points at `192.168.64.4` with
  the key `~/.ssh/codync_windows_vm`.
- **GUI apps** must start in the signed-in session (session 2):
  `schtasks /create /tn X /tr <exe> /sc once /st 23:59 /it /f; schtasks /run /tn X`.
- **Screen** without touching the Mac's UI: QEMU VNC on `127.0.0.1:5905`, e.g.
  `uvx --from vncdotool vncdo --force-caps -s 127.0.0.1::5905 capture x.png`. Input lags a few
  seconds; `--force-caps` is needed for symbols.
- The Mac sleeping pauses the VM: wrap long installs in `caffeinate -i`.
- Rebuilding the VM: the unattended install assets live in `~/VMs/codync-windows/` (about 20
  minutes). NVMe must be the first drive and both ISOs detached afterwards, or Windows reinstalls on
  every boot.

## Testing a build

1. Install the release under test (`codync-windows-x64.exe` from the GitHub release). The first
   launch asks Defender Firewall about `codync-host`; the host then starts at sign-in.
2. **Turn analytics off on the test host before anything else**, so the VM never counts as a user:
   `setAnalytics {"enabled": false}`.
3. Drive the host over SSH instead of the GUI where possible. The API is loopback-only, which SSH
   satisfies:

   ```powershell
   $t = Get-Content "$env:USERPROFILE\.codync\token"
   Invoke-RestMethod -Method Post -Uri http://127.0.0.1:19222/api/refreshBackends `
     -Headers @{ Authorization = "Bearer $t" } -ContentType application/json -Body '{}'
   ```

   `createBot`, `send` and `history` cover most agent checks; a failed start shows up as an error
   notice in `history`. Host logs are under `%USERPROFILE%\.codync`.
4. Agents: *Install* in the agent sheet runs the CLI's native installer. Agents whose ACP adapter
   comes from npm (Claude, Codex, Gemini, Qwen, …) need Node.js: without one on the VM, Extensions
   shows *Add Node.js*, which puts Codync's own copy in `%USERPROFILE%\.codync\node` (see
   [agent setup](../features/marketplace.md)). To test the computer's own Node instead,
   `winget install OpenJS.NodeJS`.

## Limits

- An unreleased host can't be built on the Mac for Windows (its C dependencies need a MinGW
  toolchain that isn't installed). CI builds Windows only for pull requests and tags
  (`.github/workflows/host.yml`), so a fix is usually reproduced by hand in the VM first (running
  the exact command the host would) and checked on Windows again with the next release.
- `cfg(windows)` code can still be compile-checked on the Mac by copying it into a scratch crate and
  running clippy with the pinned toolchain's `--target x86_64-pc-windows-gnu`.

Investigations done this way: [npm agents failing to start on Windows, 2026-10-14](../archive/windows-npm-agents-2026-10-14.md).
