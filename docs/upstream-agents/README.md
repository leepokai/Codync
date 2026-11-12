# Upstream agent bugs

Codync drives every agent through its ACP adapter (`host/src/agent/acp.rs`, `host/src/agent/bot/`). Some adapters and
the agents behind them have bugs, ignore part of the protocol or behave in their own way. This folder records them, one
page per agent: what goes wrong, how the host patches around it (or that it doesn't), where upstream tracks it, and when
the patch can go. When enough of an agent's rows are gone, we can reconsider how Codync uses it, or replace it with
another implementation.

Last checked: **2026-11-02**. Versions in the ACP registry then: claude-acp 0.99.1, codex-acp 2.5.0, pi-acp 0.0.34,
cursor 2026.10.22, gemini 0.66.0, github-copilot-cli 1.0.105, grok-build 1.0.73, kilo 7.8.18, opencode 2.2.5,
qwen-code 0.26.5.

## Pages

| Agent | Page |
|---|---|
| Claude Code (claude-agent-acp) | [claude-agent-acp.md](claude-agent-acp.md) |
| Codex (codex-acp) | [codex-acp.md](codex-acp.md) |
| pi (pi-acp) | [pi-acp.md](pi-acp.md) |
| Cursor | [cursor.md](cursor.md) |
| CodeBuddy and WorkBuddy | [codebuddy-workbuddy.md](codebuddy-workbuddy.md) |
| Antigravity | [antigravity.md](antigravity.md) |
| DeepSeek Harness | [deepseek-harness.md](deepseek-harness.md) |
| Grok Build | [grok-build.md](grok-build.md) |
| Hermes | [hermes.md](hermes.md) |
| Kilo | [kilo.md](kilo.md) |
| oh-my-pi | [oh-my-pi.md](oh-my-pi.md) |
| OpenCode | [opencode.md](opencode.md) |
| Qwen Code | [qwen-code.md](qwen-code.md) |
| Gaps in ACP itself, covered for every agent | [acp-protocol.md](acp-protocol.md) |

## Rules

- **A new patch gets a row in the same change.** Its code comment names the defect and the upstream issue, if
  there is one. If upstream has no issue, open one, or note in the row why we didn't.
- **A bug we don't patch gets a row too**, marked **Not patched**, with what users see. The next person to hit it
  then finds it here instead of investigating it again.
- **One page per agent.** An agent's first row starts its page; add the page to the table above.
- **Remove a patch with its row.** Do it once the fixed version is the one Codync starts: the registry build
  (`~/.codync/registry.json` pins the version), or the oldest CLI we still support for harnesses that run locally. If
  a fix exists but users can still run an older version, keep the patch and write "fixed in X, keep until Y" in the
  row.
- **Check upstream when an agent's registry version moves**, and whenever a cited issue or PR closes:
  `gh issue view <n> -R <owner/repo>`, `gh pr view <n> -R <owner/repo>`. Then retest the defect with the new
  version before deleting anything, and update "Last checked" above.
- Workarounds that only parse agent-specific fields (error categories, auth `_meta`) are cheap. They are listed
  so they don't surprise anyone, but there is no hurry to remove them.
