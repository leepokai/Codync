# Agent sessions: ids, titles, hooks and injection

Research for giving every coding agent on a computer a Network mailbox keyed by its session id
([codync-network](../features/codync-network.md), [what's left](codync-network-next.md)). For each agent:
where an MCP server learns the session it serves, where the conversation's title lives, which hooks run, and how a
message can be put into the session. Checked on macOS on 2026-11-24.

**Evidence** in each cell:

- **ran**: seen on this machine. A probe MCP server (logs its environment, `initialize` and every `tools/call`)
  and probe hooks (log their stdin) were added to the agent's config, the agent was run headless with one prompt,
  and the config was removed again.
- **file**: read from the agent's own files on this machine, not exercised.
- **ref**: claimed in the design notes of another Apache-2.0/MIT open-source project that does the same, not checked
  here.
- **—**: agent not installed here.

## Versions checked

| Agent | Catalog id | Version | Probe run |
|---|---|---|---|
| Claude Code | `claude` | 2.1.331 | ran |
| Codex | `codex` | 0.167.0 | ran |
| Cursor CLI (`cursor-agent`) | `cursor` | 2026.10.22 | ran |
| Copilot CLI | `copilot` | 1.0.114 | ran |
| Antigravity CLI (`agy`) | `antigravity` | 1.4.9 | ran |
| pi | `pi` | 1.4.0 | ran |
| opencode | `opencode` | 1.19.5 | `initialize` only: the model call failed (provider auth), so no `tools/call` |
| Grok CLI | `grok` | 1.0.77 | not run in the first probe (not signed in); later run on aimock for its MCP identity and hooks (rows below marked ran) |
| Gemini CLI | `gemini` | — | — |
| Amp | `amp` | — | — |

## Session id: where an MCP server learns it

| Agent | `clientInfo.name` | Env of the MCP child | Per call (`tools/call` `_meta`) | Hooks (stdin) |
|---|---|---|---|---|
| Claude Code | `claude-code` (ran) | `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_PID` (ran) | `claudecode/toolUseId` only (ran) | `session_id` (ref) |
| Codex | `codex-mcp-client` (ran) | none (ran) | `sessionId` = `threadId` (also `x-codex-turn-metadata.session_id`, `turn_id`) (ran) | `session_id` (ref) |
| Cursor | `Cursor` (ran) | none (ran) | none (ran) | `conversation_id`, `session_id`, `transcript_path` (ran) |
| Copilot CLI | `copilot-cli` (ran) | `COPILOT_AGENT_SESSION_ID` (ran) | `com.github.copilot/toolCallId` only (ran) | `sessionId` (ran) |
| Antigravity | `antigravity-client` (ran) | none (ran) | `antigravity.google/conversation_id`, `…/root_conversation_id` (ran) | unverified |
| pi | `pi` (ran) | `PI_CODING_AGENT=true`, no id (ran) | none (ran) | no hooks; an extension knows the session (ref) |
| opencode | `opencode` (ran) | `OPENCODE=1`, `OPENCODE_PID`, no id (ran) | `ai.opencode/sessionID` on 2.x (ref); 1.x unverified (no `tools/call` ran) | no hooks; a plugin knows the session (ref) |
| Grok CLI | `grok-shell-<server name>` (ran) | `GROK_SESSION_ID` (ran) | none needed | `session_id`, the same id (ran) |
| Gemini CLI | — | none; `GEMINI_SESSION_ID` only in hook env (ref) | — | `session_id` (ref) |
| Amp | — | — | — | plugin (ref) |

What this means for registration:

- **Inherited environment.** The pi, Antigravity, opencode and Copilot MCP children all carried the *parent*
  Claude Code session's `CLAUDE_CODE_SESSION_ID`, `CLAUDECODE` and inbox socket, because those agents were started
  from a Claude Code shell (ran). So the kind comes from `clientInfo.name`, and an env id counts only for that
  kind: an agent started from a Claude shell must not register as that Claude session.
- **When the id is known.** At spawn: Claude Code, Copilot CLI, Grok (env; Grok ran on aimock). Only with the first
  tool call: Codex, Antigravity (`_meta`). Only through hooks: Cursor (and Gemini per ref). Not at all without our
  own extension or plugin: pi; opencode 1.x too unless its calls carry the 2.x `_meta` (unverified: rerun the probe
  signed in).
- **One MCP process, several sessions.** A server keyed by `_meta` can see several sessions over its life (Codex
  threads, Antigravity conversations); the id is read per call, not once.

## Title, busy/idle and renaming

| Agent | Title source | Busy/idle | Renamed from outside |
|---|---|---|---|
| Claude Code | `~/.claude/sessions/<pid>.json` `name` (with `nameSource`, `formerNames`); transcript `custom-title` / `ai-title` entries (file) | same file, `status` `idle`/`busy` (file) | no API found; `/rename` in the session (unverified) |
| Codex | `~/.codex/session_index.jsonl` `thread_name` by thread `id` (file) | rollout events (ref) | unverified (`codex queue --thread` also takes an exact thread name, file) |
| Cursor | `~/.cursor/chats/<hash>/<agentId>/store.db`, table `meta`, hex-encoded JSON `name` (`New Agent` until named) (file) | hooks only | unverified |
| Copilot CLI | `~/.copilot/session-state/<id>/workspace.yaml` `name`: the first prompt's text, needs cutting (file) | hooks (`userPromptSubmitted` / `agentStop`) | unverified |
| Antigravity | `~/.gemini/antigravity-cli/conversation_summaries.db` `conversation_summaries.title` (file) | same row, `status` (file) | unverified |
| pi | session `.jsonl` under `~/.pi/agent/sessions/`, `session_info` entry `name`: first prompt's text, needs cutting (file) | extension events `agent_start` / `agent_settled` (ref) | unverified |
| opencode | `~/.local/share/opencode/opencode.db` `session.title` (`New session - <date>` until titled) (file) | plugin events (ref) | unverified |
| Grok CLI | `~/.grok/sessions/<cwd>/<id>/`, format unverified | hooks (ref) | unverified |
| Gemini CLI, Amp | — | — | — |

Every title above is readable as a file or SQLite row, so the host can read it and cache it; none needs the agent's
cooperation. No agent was found with a supported way to rename a session from outside, so naming a mailbox
(`set_name`, built in phase 1) changes only the Network address, never the agent's own title.

## Hooks (turn tier)

| Agent | Where | Events seen | Inject at the next tool call / at stop |
|---|---|---|---|
| Claude Code | `~/.claude/settings.json` | (not needed: push through the inbox socket, already built in `reach.rs`) | — |
| Codex | `~/.codex/hooks.json` | `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop` fired in `codex exec` (ran); `session_id` = the MCP `_meta.sessionId` (ran) | `PostToolUse` `hookSpecificOutput.additionalContext` (a top-level field makes Codex drop the whole answer), `Stop` `decision:block` + `reason` (ran); hooks need a one-time approval in `/hooks` (`--dangerously-bypass-hook-trust` for a test run) |
| Cursor | `~/.cursor/hooks.json`, project `.cursor/hooks.json` | `sessionStart`, `postToolUse` fired in `-p`; `beforeSubmitPrompt` and `stop` were configured but did not fire in `-p` (ran) | `postToolUse` `additional_context`, `stop` `followup_message` (ref) |
| Copilot CLI | `~/.copilot/hooks/*.json` (project `.github/hooks/` only in a trusted folder, ran) | `sessionStart`, `userPromptSubmitted`, `postToolUse`, `agentStop`, `sessionEnd` (ran); PascalCase names (`PostToolUse`, `Stop`) get Claude-style stdin with `session_id` = `COPILOT_AGENT_SESSION_ID` (ran) | top-level `additionalContext` (not `hookSpecificOutput`), `Stop` `decision:block` + `reason` (ran) |
| Grok CLI | `~/.grok/hooks/*.json` (file) | `SessionStart`, `PostToolUse`, `Stop` fired in `grok -p`, with both `hook_event_name` and `hookEventName` (ran) | `PostToolUse` `hookSpecificOutput.additionalContext`, `Stop` `decision:block` + `reason` (ran); SessionStart and prompt-submit output dropped (ref) |
| Gemini CLI | `~/.gemini/settings.json` `hooks` | — | `BeforeAgent` / `AfterTool` `additionalContext`, `AfterAgent` deny (ref) |
| Antigravity | unverified (`~/.gemini/settings.json` holds Gemini CLI's hooks, not `agy`'s) | — | native hooks per ref, unverified |

Gotcha (ref): Cursor and Copilot CLI also load Claude Code's hooks from `~/.claude/settings.json`, so a hook entry
must check which agent is calling it.

## Plugins and extensions (push tier)

| Agent | Where | Injection API |
|---|---|---|
| pi | `~/.pi/agent/extensions/*.ts` (file) | `sendUserMessage(text, {deliverAs: "steer"})` mid-run, a new turn when idle: present in the installed 1.4.0 (`core/agent-session.js`) |
| opencode | `~/.config/opencode/plugins/*.ts` (file) | `client.session.promptAsync` on 1.x, `ctx.session.prompt` on 2.x (ref; not checked in 1.19.5) |
| Amp | plugin (ref) | `thread.appendUserMessage(…, {steer: true})` (ref) |
| Claude Code | inbox socket `CLAUDE_CODE_MESSAGING_SOCKET` (ran, built) | `{"type":"user",…}` frame |
| Codex | `codex queue --thread <id> --message` (file, built) | queued as the thread's next input |

## MCP config locations (for setup)

| Agent | User scope | Project scope | Command |
|---|---|---|---|
| Claude Code | `~/.claude.json` | `.mcp.json` | `claude mcp add -s user` |
| Codex | `~/.codex/config.toml` `[mcp_servers.<name>]` (file) | `.codex/config.toml` `[mcp_servers.<name>]`, trusted projects only (ref) | `codex mcp add` (user scope) |
| Cursor | `~/.cursor/mcp.json` (file) | `.cursor/mcp.json` (ran) | — |
| Copilot CLI | `~/.copilot/mcp-config.json` (file) | — | `--additional-mcp-config` per run (ran) |
| Grok CLI | `~/.grok/config.toml` `[mcp_servers.<name>]` (file) | `.grok/config.toml` | `grok mcp add -s user` / `-s project` |
| opencode | `~/.config/opencode/opencode.jsonc` `mcp` (file) | `opencode.json` (ran) | — |
| pi | `~/.pi/agent/mcp.json` (ran) | `.pi/mcp.json`, trusted projects only | `pi mcp add` / `remove` (ran) |
| Antigravity | `~/.gemini/config/mcp_config.json` (file) | — | `agy mcp add` / `remove` (ran) |
| Gemini CLI | `~/.gemini/settings.json` `mcpServers` (ref) | `.gemini/settings.json` | `gemini mcp add` |

The `add`/`remove` commands that exist (Claude, Codex, Grok, pi, Antigravity) are the safest way to change a
user's config: they own the file format. The others need a JSON merge that touches only our own key.

## ACP: a session's title for Codync's own bots

ACP has `session_info_update` with `title`, `updatedAt` and `_meta` (all optional): present in the
`@agentclientprotocol/sdk` 0.26.0 schema, the oldest copy on this machine (the version that introduced it isn't
pinned). Adapters that send a `title`: codex-acp 2.7.0 (the thread name) and claude-agent-acp 0.100.0
(`session-titles.js`). claude-agent-acp also sends `session_info_update` with only `_meta` (goals, prompt
suggestions). `host/crates/agent/src/events.rs` keeps it as `Event::Other` today. Codync bots already have names,
so this only matters if a bot's conversation title is ever shown.

## Delivery tier per agent (proposed)

| Tier | Agents |
|---|---|
| push (wakes idle) | Claude Code: built, through the session's own MCP server (its inbox socket and token), once idle; checked in an interactive session on aimock (an idle session takes it and starts a turn; one written mid-turn didn't reach the turn). Follow-ups: pi and opencode (their MCP calls carry no session id; opencode 1.19.5 sends no `_meta` id, ran), Amp (not installed), Claude Code on Windows (named pipe) |
| queued (idle wake unverified) | Codex (`codex queue`): queued as the thread's next input; whether it starts a turn on an idle thread without user input is not checked yet |
| turn (next tool call or stop) | Codex, Copilot CLI and Grok: built (`codync-host network hook`), checked end to end on aimock. Cursor: its MCP calls carry no session id, so a hook can't find the mailbox (follow-up); Gemini CLI and Antigravity not installed / unverified (follow-up) |
| pull (`check_inbox` + an unread notice, built in phase 1) | any MCP client, including the above before their hooks or plugins are installed |

The network view's sessions take their tier from the session's saved `tier`, and every client labels `pull`, `turn` and `push`.

## Open design questions

Phase 1 settled the first three as proposed ([feature](../features/codync-network.md#coding-agents-sessions)):
`whoami` and an address in every tool result, temporary sessions keyed by the MCP server's process (`process:<pid>-<nonce>`),
and a registry keyed by `(kind, session id)` per call.

1. **"The server tells the agent its address when it connects."** Only Claude Code and Copilot CLI have their session
   id at `initialize`. Codex and Antigravity show it with the first tool call, Cursor only through hooks, pi not at
   all, opencode 1.x unverified. Proposal: the `initialize` instructions say to call a new `whoami` tool (or read the
   first tool result, which names the address); the address is fixed at the first call that knows the session.
2. **Agents with no session id over MCP** (pi and opencode without our extension, Cursor without hooks, any other
   MCP client). Proposal: key them by the MCP process instance (`<pid>-<nonce>`, a fresh nonce for each process, so a
   reused pid never gets an old mailbox), so the address lasts while that instance runs and a restart gets a new
   one; the hooks/plugins of later phases give them real ids.
3. **One MCP process, several sessions** (Codex, Antigravity): the registry is keyed by `(kind, session id)` per
   call, not per process. Cursor's calls carry no id, so its sessions are told apart only through hooks.

## Other catalog agents

qwen, hermes, goose, kimi, droid, kilo, cline, auggie, vibe, dsh, kiro, devin, qoder, codebuddy, workbuddy, minimax,
junie, cortex, poolside and omp aren't installed here: they get the pull tier as plain MCP clients, and their kind
comes from `clientInfo.name` once someone records it. Follow-up per agent.
