# Windows testing

Codync ships x64 and ARM64 Windows apps (Electron + the matching bundled `codync-host.exe`). Real Windows behavior is
checked in a UTM virtual machine on the Mac. Everyday end-to-end checks stay on the Mac (minimal
e2e); use the VM for a full e2e, or when a change or a bug depends on Windows itself (paths,
`cmd.exe`, `.cmd` shims, the service, timing). The VM slows the whole Mac down: start it for the
check and stop it as soon as the check is done.

Host CI runs `cargo nextest run --workspace --profile ci` on every OS (`host/.config/nextest.toml`):
each test runs in its own process, so fixture hosts no longer contend on process-global memory scan
locks and Windows runs them in parallel too. A test that fails and then passes on a retry is
reported as FLAKY instead of failing the run: fix it rather than lean on the retry.

Desktop release CI builds x64 on `windows-latest` and ARM64 on `windows-11-arm`, then smoke-tests
each installed host after running the installer and checking the native app/host executable headers.
Installers, checksums and blockmaps use `codync-windows-<arch>.exe` names.
Both builds contribute to one `latest.yml` update feed, uploaded only after both installers;
electron-updater selects the file matching the installed app's architecture. The pinned computer-use
driver and UI Automation helper remain x64 and run through Prism on ARM64 Windows 11.
Windows packaging forces the supported BCJ archive filter: the bundled NSIS decoder silently skips
executables compressed with modern 7z ARM64 filters, even when the installer reports success.
For a manual CI check, dispatch **Release desktop** with `windows_only` enabled to build both
Windows architectures without requiring Mac signing secrets or running Linux packaging.

## The VM

- **UTM "Codync Windows"**: Windows 11 ARM64 (use the ARM64 build for native checks, x64 for Prism emulation),
  8 GB / 4 cores / 64 GB NVMe, account `codync` / `codync`, signs in automatically.
- **Start / stop** from the Mac, never through UTM's window:
  `osascript -e 'tell application "UTM" to start virtual machine named "Codync Windows"'`
  (`stop … by force` only when a normal stop hangs; `query ip of …` works). `utmctl` fails with
  OSStatus -10004. Pressing UTM's Run button through accessibility crashed UTM once; if UTM is in a
  bad state, quit it and reopen it in the background with `open -g -a UTM`. After stopping, end
  leftover sessions with `pkill -f "ssh codync-win"`.
- **Shell**: `ssh codync-win` (PowerShell). The `~/.ssh/config` alias points at `192.168.64.4` with
  the key `~/.ssh/codync_windows_vm`. SSH is up a minute or two after start:
  `until ssh -o ConnectTimeout=5 -o BatchMode=yes codync-win 'echo up'; do sleep 10; done`.
- **GUI apps** must start in the signed-in session (session 2):
  `schtasks /create /tn X /tr <exe> /sc once /st 23:59 /it /f; schtasks /run /tn X`.
- **Screen** without touching the Mac's UI: QEMU VNC on `127.0.0.1:5905`, e.g.
  `uvx --from vncdotool vncdo --force-caps -s 127.0.0.1::5905 capture x.png`. Input lags a few
  seconds; `--force-caps` is needed for symbols.
- **Network**: the VM's internet is slow and drops connections (2026-10-20: a 4.4 MB npm tarball
  took 27 s in the VM and 0.3 s on the Mac, about 165 KB/s; a 130 MB `Invoke-WebRequest` download
  stopped at 45 MB without an error). Download large files on the Mac and `scp` them in.
- The Mac sleeping pauses the VM: wrap long installs in `caffeinate -i`.
- Rebuilding the VM: the unattended install assets live in `~/VMs/codync-windows/` (about 20
  minutes). NVMe must be the first drive and both ISOs detached afterwards, or Windows reinstalls on
  every boot.

## Installing a release

1. Download the installer on the Mac and copy it in under a new name (`scp` can't overwrite a file
   Windows still holds open: `dest open … Failure`):

   ```sh
   gh release download v2.20.3 --repo leepokai/Codync -p codync-windows-x64.exe -p codync-windows-x64.exe.sha256 -D <dir>
   shasum -a 256 <dir>/codync-windows-x64.exe        # must match the .sha256 file
   scp <dir>/codync-windows-x64.exe codync-win:codync-2.20.3.exe
   ```

2. Stop Codync, then install silently over SSH:

   ```powershell
   Get-Process Codync,codync-host,codync-hostw -ErrorAction SilentlyContinue | Stop-Process -Force
   $i = Start-Process "$env:USERPROFILE\codync-2.20.3.exe" -ArgumentList "/S","/currentuser" -Wait -PassThru; $i.ExitCode
   & "$env:LOCALAPPDATA\Programs\codync-desktop\resources\codync-host.exe" --version
   ```

   Exit code 2 is the installer's integrity check: the file is truncated, so compare
   `(Get-FileHash <file>).Hash` with the release's `.sha256`. `taskkill` can leave the old
   binaries locked; `Stop-Process -Force` on all three processes frees them.
3. Start the app in the signed-in session (`schtasks` above, with
   `$env:LOCALAPPDATA\Programs\codync-desktop\Codync.exe`). The first launch asks Defender Firewall
   about `codync-host`; the host then starts at sign-in through the HKCU Run value `CodyncHost`
   (`codync-hostw.exe`).
4. **Turn analytics off on the test host before anything else**, so the VM never counts as a user:
   `setAnalytics {"enabled": false}`. `hello` reports the current setting (`analytics`).

## Driving the host over SSH

The API is loopback-only, which SSH satisfies. Copy this helper to the VM as `api.ps1`:

```powershell
param([string]$Method, [string]$Body = "{}")
# A body of "@file.json" is read from that file (avoids quoting JSON through ssh).
if ($Body.StartsWith("@")) { $Body = Get-Content -Raw $Body.Substring(1) }
$t = Get-Content "$env:USERPROFILE\.codync\token"
$r = Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:19222/api/$Method" -Headers @{ Authorization = "Bearer $t" } -ContentType "application/json; charset=utf-8" -Body ([System.Text.Encoding]::UTF8.GetBytes($Body))
$r | ConvertTo-Json -Depth 12
```

- Run it as `powershell -NoProfile -ExecutionPolicy Bypass -File api.ps1 <method> "@<body>.json"`:
  Windows refuses scripts by default, and JSON quoted through ssh and PowerShell arrives broken, so
  write each body to a file on the Mac and `scp` it over.
- `refreshBackends` lists agents (`installed`, `available`, `signedIn`); `agentAuth`
  (`{"backend":"claude"}`) runs the sign-in check the agent sheet shows; `createBot`
  (`{"name":…,"backend":"claude","cwd":"C:\\Users\\codync","connectors":[],"skills":[]}`), `send`
  (`{"botId":…,"text":…}`) and `history` (`{"botId":…,"limit":20}`) cover most agent checks. A
  failed start or a sign-in card shows up in `history` as a notice
  (`data.text`, `data.connectionRequest`).
- Waiting for a reply: poll `history` every few seconds until an entry that isn't `user` appears.
  Agent starts in the VM take minutes, so run the wait as a background command on the Mac rather
  than one long foreground `ssh`.
- Host log: `%USERPROFILE%\.codync\host.log` (strip colors with `-replace "\x1b\[[0-9;]*m",""`).
  Agent processes: `Get-CimInstance Win32_Process | Where-Object { $_.Name -like "node*" } |
  Select ProcessId,ParentProcessId,Name,CommandLine`.

## Agents

- *Install* in the agent sheet runs the CLI's native installer. Agents whose ACP adapter comes from
  npm (Claude, Codex, Gemini, Qwen, …) need Node.js: without one on the VM, Extensions shows *Add
  Node.js*, which puts Codync's own copy in `%USERPROFILE%\.codync\node` (see
  [agent setup](../features/marketplace.md)). To test the computer's own Node instead,
  `winget install OpenJS.NodeJS` (the VM has Node 24, ARM64).
- **Signing an agent in needs the user**: it's an interactive browser sign-in on the VM's screen,
  with their account. Ask them; never sign in on their behalf. Claude Code in the VM is signed out.
- **First npm adapter download**: Claude's adapter with its Claude binary is about 340 MB, and on
  the VM's network the host's 20-minute prefetch timed out (`couldn't download Claude Agent with
  npm: timed out`). Seed the npx cache from the Mac instead. The folder name is the first 16 hex
  digits of the SHA-512 of the package spec; `--cpu` must match the VM's Node (`node -p
  process.arch`), not Codync's x64:

  ```sh
  P=@agentclientprotocol/claude-agent-acp@0.94.0
  printf '%s' "$P" | shasum -a 512 | cut -c1-16                  # npx cache key, e.g. 1f872328d1400346
  mkdir -p <dir> && printf '%s\n' '{"dependencies":{"@agentclientprotocol/claude-agent-acp":"^0.94.0"},"_npx":{"packages":["@agentclientprotocol/claude-agent-acp@0.94.0"]}}' > <dir>/package.json
  npm install --prefix <dir> --os=win32 --cpu=arm64 --no-audit --no-fund
  COPYFILE_DISABLE=1 tar -C <dir> -czf npx-claude-win.tgz . && scp npx-claude-win.tgz codync-win:
  ```

  ```powershell
  $d = "$env:LOCALAPPDATA\npm-cache\_npx\<key>"
  New-Item -ItemType Directory -Force $d | Out-Null; tar -xzf "$env:USERPROFILE\npx-claude-win.tgz" -C $d
  Remove-Item -Recurse -Force "$d\node_modules\.bin"; cd $d; npm rebuild --offline --no-audit --no-fund
  ```

  The Mac's `node_modules\.bin` has symlinks, not Windows `.cmd` shims; `npm rebuild` writes them
  (check for `claude-agent-acp.cmd`).
- **Launches stay unreliable** even with a seeded cache: `npx -y` asks the registry on every launch
  and fetches anything missing (one 2026-10-20 launch spent 55 s on a single small tarball), so an
  adapter can sit silent past the host's start limit (`the agent didn't start within 180s`), with
  or without `--prefer-offline`. When that blocks a check, run the agent's commands by hand (next
  section) or reproduce the condition on the Mac.

## Checking an agent by hand

- **Time a CLI** the way the host or adapter runs it, e.g.
  `foreach ($i in 1..3) { $s = Get-Date; & "$env:USERPROFILE\.local\bin\claude.exe" auth status | Out-Null; ((Get-Date) - $s).TotalSeconds }`
  (2026-10-20: 5.5 to 8.8 s in the VM).
- **See what an ACP adapter reports**: a small Node script that starts the adapter's command with
  `shell: true` (for a cached npm adapter, `…\_npx\<key>\node_modules\.bin\claude-agent-acp.cmd`),
  writes one `initialize` line, and prints every line it gets back with a timestamp for 25 s
  (stderr passes through):

  ```json
  {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false,"auth":{"terminal":true},"_meta":{"terminal-auth":true}}}}
  ```

  An adapter that advertises `agentCapabilities._meta.authStatus` should follow its answer with an
  `_auth/status_update` notification.
- **npm**: run the host's command yourself with `--loglevel http` (or `silly`) and
  `-RedirectStandardError`; npm's own logs are in `%LOCALAPPDATA%\npm-cache\_logs`.

## Testing an unreleased host

Cross-build the host on the Mac (Homebrew `mingw-w64` and the pinned toolchain), from the change's
worktree:

```sh
T=~/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin
cd host && PATH="$T:$PATH" CARGO_TARGET_DIR=<scratch>/tgt-win CODYNC_ENV=main \
  CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++ \
  AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  "$T/cargo" build --locked --profile dev-app --target x86_64-pc-windows-gnu --bin codync-host --bin codync-hostw
```

The binaries land in `<scratch>/tgt-win/x86_64-pc-windows-gnu/dev-app/`. `CODYNC_ENV=main` gives
the build the installed host's identity (port 19222, `~/.codync`, the `CodyncHost` Run value), so it
replaces that host and keeps its analytics-off setting. `dev-app` is a release build and sends
analytics when they're on, so only run it on the VM's host, where they're off. Without
`CODYNC_ENV` it would be Codync Dev (19223, `~/.codync-dev`, analytics on by default).

`scp` both files in as `codync-host-fix.exe` and `codync-hostw-fix.exe`, then swap them in, keeping
the release's binaries:

```powershell
$r = "$env:LOCALAPPDATA\Programs\codync-desktop\resources"
Get-Process Codync,codync-host,codync-hostw -ErrorAction SilentlyContinue | Stop-Process -Force
if (-not (Test-Path "$r\codync-host.orig.exe")) { Copy-Item "$r\codync-host.exe" "$r\codync-host.orig.exe"; Copy-Item "$r\codync-hostw.exe" "$r\codync-hostw.orig.exe" }
Copy-Item -Force "$env:USERPROFILE\codync-host-fix.exe" "$r\codync-host.exe"; Copy-Item -Force "$env:USERPROFILE\codync-hostw-fix.exe" "$r\codync-hostw.exe"
schtasks /create /tn CodyncHostFix /tr "`"$r\codync-hostw.exe`" serve --port 19222" /sc once /st 23:59 /it /f; schtasks /run /tn CodyncHostFix
```

The first start of a swapped host took several minutes to answer `hello`; poll it for up to ten
minutes. Until the swap is undone, the test host also starts at every sign-in. To undo it, stop
the three processes and copy the `.orig.exe` files back, or reinstall the release.

## Reproducing Windows conditions on the Mac

When the VM can't run a check, reproduce the Windows condition in the worktree's desktop sandbox
(`npm run dev -- --cdp`, [worktree sandboxes](dev-app.md#worktree-sandboxes)); the host and the
agents it starts inherit the environment:

- **Signed out**: `CLAUDE_CONFIG_DIR=<empty folder> npm run dev -- --cdp`. Use a fresh folder per
  run: signing in from the sheet stores a login for that folder in the keychain (remove it with
  `CLAUDE_CONFIG_DIR=<folder> claude auth logout`).
- **A slow CLI**: Claude's adapter runs `CLAUDE_CODE_EXECUTABLE` instead of its bundled `claude`.
  Point it at a wrapper that delays one command and runs the real binary:

  ```sh
  #!/bin/sh
  # `claude auth status` taking 6 s, past the adapter's own 5 s limit (as on Windows).
  if [ "$1" = auth ] && [ "$2" = status ]; then sleep 6; fi
  exec ~/.npm/_npx/<key>/node_modules/@anthropic-ai/claude-agent-sdk-darwin-arm64/claude "$@"
  ```

  The host's own `claude auth status` uses the `claude` on PATH, so only the adapter is slowed.

## Limits

- Agent launches in the VM depend on its network (see [agents](#agents)); a check that needs an
  npm adapter to start may not finish.
- CI builds Windows only for pull requests and tags (`.github/workflows/host.yml`); a direct push
  to `dev` gets no Windows compile. Clippy the whole host for Windows on the Mac with the
  cross-build variables above and
  `"$T/cargo" clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings`.

Investigations done this way:

- [Claude reported signed in on Windows while signed out, 2026-10-20](../investigations/windows-claude-sign-in-2026-10-20.md)
- [npm agents failing to start on Windows, 2026-10-14](../investigations/windows-npm-agents-2026-10-14.md)
