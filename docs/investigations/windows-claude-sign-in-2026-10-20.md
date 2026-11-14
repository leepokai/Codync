# Claude reported signed in on Windows while signed out — 2026-10-20

> Historical record of one investigation, not a current test result. How to test on Windows:
> [Windows testing](../guides/windows-testing.md).

## Signal

PostHog (project 649427), production (`env = main`), owner and e2e test hosts excluded (cohort
618410), queried 2026-10-20:

- Claude turns failed on 28% of attempts on Windows, 8% on macOS. Only 29% of Windows users' first
  Claude turn succeeded (macOS 58%, Linux 74%).
- New computers that sent a message but got no reply on their first day: Windows 33%, macOS 15%,
  Linux 6%.
- Claude `sign_in` failures reached 57 people, and 60% of them never had a successful turn after
  it. On Windows, 5 of 18 people whose first turn failed with `sign_in` recovered.

## Reproduction

VM "Codync Windows", Codync v2.20.3 from the GitHub release, analytics off on the host, Claude Code
installed and signed out (`claude auth status`: `loggedIn: false`), Node.js 24 (ARM64).

1. A new Claude bot's first message: `Couldn't start the agent: couldn't download Claude Agent
   with npm: timed out`. The npm prefetch (`npm exec --yes --package
   @agentclientprotocol/claude-agent-acp@0.94.0 -c "exit 0"`) ran into the host's 20-minute limit on
   the VM's network. The adapter's npx cache was then seeded from the Mac (see
   [agents](../guides/windows-testing.md#agents)).
2. Sent again: the bot showed the agent sign-in card (`Needs you to sign in. Your message goes out
   again once you're signed in.`), which is right.
3. `agentAuth {"backend":"claude"}` (the agent sheet's check) took 72 s and returned
   `signedIn: true` with both sign-in methods. So the sheet said *Signed in.* with no sign-in
   button, and tapping done on the card completed it and resent the message, which failed again:
   a loop the user can't leave.
4. `claude auth status` in the VM took 8.8, 6.1 and 5.5 s on three runs.
5. Sending `initialize` to the adapter by hand
   ([checking an agent by hand](../guides/windows-testing.md#checking-an-agent-by-hand)): the answer
   came after 5.0 s and advertised `agentCapabilities._meta.authStatus`, stderr said `claude auth
   status did not answer within 5 s; the identity is not refreshed`, and no
   `_auth/status_update` followed within 25 s.

## Root cause

Claude's adapter reports the login only if its own `claude auth status` answers within 5 s; on
Windows (x64 under emulation in the VM) it takes longer, so the adapter stays silent. The host's
check (`auth::probe`, `host/src/agent/auth.rs`) then fell back to `session/new`, the test it uses
for agents that don't report a login. Claude's adapter opens a session without checking the login,
so the check said signed in.

## Fix

f95b9fff (2.21.0): an agent that advertises `authStatus` is never judged by a session. When its
report doesn't come, the harness's own CLI check (`claude auth status`, 15 s, with the same
environment the agent gets) decides, and when there is none the answer is "can't tell", so the
sheet keeps offering sign-in. The same release added ad36a247 (the sign-in card before the first
message) and 3dc8d56a (`turn_finished` failure categories), which make the next round of this
signal readable.

## Verification

- Done: unit tests for the new rule (a report wins, no report and no CLI check means "can't tell",
  agents without `authStatus` still use a session).
- Done on the Mac, in a worktree desktop sandbox: the Windows condition reproduced with a
  `CLAUDE_CODE_EXECUTABLE` wrapper that delays `auth status` by 6 s and an empty
  `CLAUDE_CONFIG_DIR`
  ([reproducing Windows conditions](../guides/windows-testing.md#reproducing-windows-conditions-on-the-mac)).
  `agentAuth` took 9 s and returned `signedIn: false` with the sign-in methods; the sheet said *Not
  signed in yet.* with its options; a new Claude bot got the *Needs you to sign in before your
  first message.* card and it stayed pending, with no resend loop. Signed in with the same slow
  wrapper, the check said signed in (10 s), and a normal signed-in bot answered.
- Not done in the VM: the cross-built fixed host ran there, but its check never finished. The
  adapter launch sat silent on the VM's network (`the agent didn't start within 180s` after
  392 s). After 2.21.0 ships, check in PostHog that Windows Claude `sign_in` failures and
  first-turn success improve, and repeat step 3 in the VM when its network allows.

## Seen, not fixed

- `npx -y` asks the registry on every launch and fetches anything missing (one launch in the VM
  spent 55 s on a single small tarball), so an adapter that is already cached can still fail to
  start on a slow or offline network. `--prefer-offline` didn't change it in the VM.
- A first Claude message downloads about 340 MB (the adapter and its Claude binary). On a slow
  link the user waits up to the 20-minute prefetch limit before seeing an error.

VM state left behind: Codync 2.20.3 installed, but `resources\codync-host.exe` and
`codync-hostw.exe` are the cross-built test host (2.20.4, an early version of the fix), with the
release's binaries saved as `codync-host.orig.exe` and `codync-hostw.orig.exe`; restore them
before testing a release. Also: scheduled tasks `CodyncSetup`, `CodyncSetupUI`, `CodyncApp` and
`CodyncHostFix`; in `C:\Users\codync` the helpers (`api.ps1`, `wait-reply.ps1`, `wait-more.ps1`,
`wait-host.ps1`, `acp-auth-probe.mjs`, JSON bodies), `codync-2.20.3.exe`, the `*-fix.exe` builds and
`npx-claude-win.tgz`; the seeded npx cache `1f872328d1400346` (claude-agent-acp 0.94.0); test bot
`WinClaude`; Claude Code still signed out.
