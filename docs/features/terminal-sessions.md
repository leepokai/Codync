# Terminal agent sessions

Codync can watch the coding agents you run yourself, in your own terminal: Claude Code, Gemini CLI,
Antigravity, Codex and Copilot CLI. Those sessions become bots in the roster — status, tool calls with their
diffs, approval cards and pushes — without moving your work into Codync.

The hook design follows [Coucou](https://github.com/Louis-CFM/coucou) (MIT): a hook never blocks the agent,
and nothing that allows anything is ever sent back without a person choosing it.

## Turning it on

```sh
codync-host hooks install    # adds the hook to ~/.claude/settings.json
codync-host hooks status
codync-host hooks uninstall  # takes only Codync's hook out again
```

Your own hooks in that file are kept as they are. Agents other than Claude Code need the same command in
their own hook configuration, with `--agent <name>`:

```sh
codync-host hook --agent gemini
```

The hook runs for every event Claude Code offers: `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
`PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `Stop`, `SessionEnd`.

Codync's own bots are not affected: their processes carry `CODYNC_BOT=1`, and the hook exits at once for
them.

## What you see

- **One bot per terminal session**, named `<agent> · <folder>` (e.g. `Claude Code · space-folder`), backend
  `external`, with its own chat. Its working folder is the session's.
- **Status**: `working` with the running tool ("Run cargo test", "Edit parser.rs"), `needsInput` while an
  approval waits, `idle` after `Stop`.
- **Transcript**: your prompt, every tool call (completed with its changed lines for `Edit` / `MultiEdit` /
  `Write`), the agent's last message when a turn ends, and the approval cards.
- **Approvals** on every client, including the desktop island and phone notifications. The same card answers
  once; a late tap is a no-op.
- **Finished sessions leave the roster**: `SessionEnd` removes the bot, and so does a host restart (a session
  the host stopped watching can never be honored again). A session with no events for 6 hours is a terminal
  that was closed without a goodbye.

The chat of one of these bots is read-only: the composer says the session runs in your terminal (desktop and
Android), and the host refuses a message with a pointer there. The conversation belongs to the agent that owns
it; the approval cards are the part Codync answers.

## Approvals on each client

| Client | What a person does |
|---|---|
| Android | **Allow** on a command or a file change asks for the fingerprint, face or the device PIN first; **Deny** is one tap. Taps in the first 600 ms after the screen lights up are ignored — that is the pocket picking the phone up. A phone with nothing enrolled approves as before. |
| Desktop (macOS, Linux, Windows) | The card in the chat and in the island. No second factor: the desktop already needs the person's unlocked session, and there is no biometric to ask for. |
| Terminal UI | The card's option keys, as for a bot's own alert. |
| iPhone | The card in the chat, as for a bot's own alert: this change adds no extra check and leaves that client's composer as it was (typing there gets the host's "reply in your terminal"). |

Android's prompt lives in `apps/android/app/src/main/kotlin/com/codync/android/Approvals.kt`, and both the chat
card and the roster's quick answer go through it.

## The rules that keep the terminal usable

- **Never block the agent.** Reaching the host has a 300 ms budget; if the host is not there, the hook exits 0
  in silence and the agent asks in its terminal as usual. A host that accepts a connection and then stops
  reading cannot wedge the session either: each event has an overall deadline (2 s; 115 s for an approval).
- **No decision, no allow.** The host answers only with `allow` or `deny` chosen on a client. With nobody
  answering, the card expires after 108 s and the hook prints nothing, so the terminal shows its own prompt.
- **Only agents that can be answered** make approval cards: Claude Code, Codex and Copilot CLI. Gemini CLI
  and Antigravity have no permission event to answer; their events still update the roster, and their hook
  reply is `{}` ("no opinion") — or `ask` for Antigravity's `PreToolUse`, which otherwise reads `{}` as a
  denial.
- **Payload limits.** Every string is cut at 2 000 bytes, except the edit text of a finished `Edit` /
  `MultiEdit` / `Write` (256 KB each, 512 KB per event) which the chat diff needs; a cut edit sets
  `diffTruncated` so the host never shows counts it cannot trust. `tool_response`, `tool_output` and
  `transcript_path` are never forwarded.
- **Same door as the rest of the local API**: the loopback bearer token in `~/.codync/token`, checked before
  anything else.

## How it fits together

| Piece | Where |
|---|---|
| The relay an agent runs | `codync-host hook` (`host/src/cli/hook.rs`), POSTing to `/ingest/hook` |
| Event and field names per agent | `host/src/external/normalize.rs` |
| Each agent's stdout contract | `host/src/external/reply.rs` |
| Session → bot, status and transcript | `host/src/external/session.rs` |
| Approval cards and the waiting hook | `host/src/external/permission.rs` |
| Installing the hook in Claude Code | `host/src/external/install.rs` |
| The bot that stands for a session | `host/src/hub/terminal.rs` |

Because a session is a bot, every client already renders it: no client has its own copy of this logic.

## Known limits

- Tool calls show only once they are complete in the transcript (`in_progress` while they run, as with a
  bot's own calls); a `PreToolUse` that never completes keeps its `in_progress` row.
- A question Claude Code asks (`AskUserQuestion`) is not answered from Codync yet: the terminal shows it, and
  the agent continues when you answer there.
- The hook is installed per agent and per computer; Codync does not edit settings of agents other than Claude
  Code.
- Deleting one of these bots from the roster stops Codync watching that terminal session until the terminal
  starts a new one (a `SessionEnd` or a fresh session id). A permission card waiting at that moment is closed
  and the terminal asks there, at once.
- **Stop** and **New session** do nothing for a terminal session: the running agent is interrupted from its
  own terminal.
- Windows: the installed command uses a forward-slash path in double quotes, which Git Bash and PowerShell
  both read; Claude Code's own shell must be one of those.

## Questions and plans from a terminal session

`codync-host hooks install` adds a second `PreToolUse` hook (`codync-host hook --ask`, matcher
`AskUserQuestion|ExitPlanMode`, 120 s). When Claude Code asks a question or presents a plan, the host shows a card on
every client (a Permission entry with `toolKind` `question` or `plan`):

- **Question:** one card per question (up to 4), its options are the question's own (2 to 4). The picked labels go back
  as `updatedInput.answers`, which is how a hook answers `AskUserQuestion`. A multi-select question, one with other
  than 2 to 4 options, or more than 4 questions stays in the terminal.
- **Plan:** an Approve / Reject card showing the plan. Reject denies the tool with a reason; Approve allows it with the
  input unchanged.
- An answer must be one of the options the card showed; anything else is ignored and the card keeps waiting. With no
  answer in time, a cancel, or an older install without the `--ask` hook, the terminal asks as before.
- Rerun `codync-host hooks install` after updating to add the new hook.
