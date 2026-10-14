# npm agents failing to start on Windows — 2026-10-14

> Historical record of one investigation, not a current test result. How to test on Windows:
> [Windows testing](../guides/windows-testing.md).

## Signal

PostHog `turn_finished`, 14 days to 2026-10-14, production builds, internal accounts and automated
test hosts excluded:

| Agent on Windows | Start failures | Successful turns |
|---|---|---|
| Claude (2.14.0–2.15.2) | 19, from 12 users | 2, from one user |
| Claude, 2.15.2 only | 5, from 3 users | 0 |
| Codex | 11 | 4 |
| Antigravity, 2.15.2 | 5 | 0 |

Agents shipped as binaries (OpenCode, Grok) worked on Windows in the same period. The failure kind
wasn't recorded yet (`turn_finished.error` came later), so the cause had to be reproduced.

## Reproduction

VM "Codync Windows", Codync v2.16.0 from the GitHub release, analytics turned off on the host.

1. **No Node.js**: a Claude bot fails with `Couldn't start the agent: Claude Agent has no build for
   this computer (needs Node.js)`. The registry's `claude-acp@0.90.0` and `codex-acp@2.2.2` are
   npx-only.
2. **Claude Code installed** with the host's own Windows command
   (`irm https://claude.ai/install.ps1 | iex`): the agent list reports `installed: true,
   available: false`, the picker still says "(not installed)" with the hint "Install Claude Code
   and run `claude auth login`", and sending gives the same "needs Node.js" error.
3. **Node.js 24 installed** (`winget`): `Couldn't start the agent: couldn't download Claude Agent
   with npm: npm notice`. Running the host's prefetch by hand from `%USERPROFILE%\.codync`,
   `npm exec --yes --package @agentclientprotocol/claude-agent-acp@0.90.0 -c true`, prints
   `'true' is not recognized as an internal or external command, operable program or batch file.`
   and exits 1. The package itself had downloaded; the host then deletes the half-prepared cache
   folder and fails, so every message repeats the download and the failure.
4. **Same prefetch with `-c "exit 0"`**: exits 0 and leaves `package.json` in the cache. The Claude
   bot then answers `Authentication required` and Codex `Codex needs you to sign in…`: the expected
   sign-in states, so the rest of the launch works.

## Root cause

`prepare_npx` (`host/src/agent/registry.rs`) downloads an npm adapter with
`npm exec --yes --package <pkg> -c true`. npm runs `-c` through `cmd.exe` on Windows, which has no
`true` command, so the download always "failed" there. The line dates from f5fa9eae (v2.2.2), so
every Windows release had it. The visible error was npm's update notice because the host reported
the last line of stderr.

## Fix

7747a378 (2.16.1): the prefetch runs `-c "exit 0"`, which both `sh` and `cmd.exe` understand, and a
failed download reports the last stderr line that isn't an `npm notice`.

## Verification

- Done: a unit test for the error line; on the Mac, `npm exec … -c "exit 0"` exits 0 (and
  `"exit 3"` exits 3); a debug host with an empty npm cache downloaded the Claude adapter through the
  new prefetch, started it, and the bot replied.
- Done in the VM by hand: steps 3 and 4 above, running the exact npm command.
- Not done: the 2.16.1 host itself on Windows (no local Windows build; see
  [limits](../guides/windows-testing.md#limits)). After 2.16.1 ships, repeat step 4 with a Claude
  bot in the VM, and check that Windows `turn_finished` with `stop_reason = start_failed` drops in
  PostHog.

## Seen, not fixed

- Without Node.js, an npx-only agent couldn't start at all, and its install hint named the harness
  setup (Claude Code), which was already done. Resolved in the same release: Extensions and the
  agent's setup offer to add Codync's own Node.js ([agent setup](../features/marketplace.md)).
- Every new bot on Windows posts `Memory is unavailable; … installing Engram: Access is denied.
  (os error 5)`.

VM state left behind: Codync 2.16.0 installed as the service (analytics off), Claude Code 2.1.299,
Node.js 24.20.0 (arm64), test bots `ClaudeTest` and `CodexTest`, and the claude-acp and codex-acp
npx caches.
