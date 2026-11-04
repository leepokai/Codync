# Topic 3: forbidden commands

## What changed, by file

- `host/src/agent/blocklist.rs` (new): the list and `blocked(command) -> Option<pattern>`. Ported from Mercury Agent's
  `src/capabilities/shell/blocklist.ts` (MIT, Copyright (c) 2026 Cosmic Stack), including the Windows delete patterns it
  generates (`Remove-Item`, `rm`, `ri`, `rmdir`, `rd`, `del`, `erase` against a drive root, `~`, `/`, in four argument
  orders). Added, marked in the file: `rd`/`rmdir`/`del`/`erase` with any flags against a drive root, `rm` flag-order and
  `--no-preserve-root` variants.
- `host/src/agent/approval_rules.rs`: the one matcher is now `glob(pattern, text, single)`; `matches` calls it with
  `single = false`, so the user's rules are unchanged. The blocklist asks for `single = true` (`?` is one character).
- `host/src/agent/bot/updates.rs` + `refuse.rs` (new): `on_permission` checks the command before the user's rules and the
  `auto` setting and answers with the reject option (a cancel when none) and a notice naming the pattern. The old `deny`
  rule path uses the same `refuse`.
- `host/src/chat/open_items.rs`: a background `shell_job` start is refused. `host/src/external/permission.rs`: a terminal
  agent's hook gets `Deny` with no card.
- `THIRD-PARTY-NOTICES.md` (new): Mercury's MIT notice. `docs/features/approvals.md`: the section "Forbidden commands".
- Tests: `blocklist` (every entry refuses itself; Unix and Windows wipes; chains, wrappers, quotes; the pattern is named),
  `approval_rules` (`?`), `external` (a forbidden command gets no card; an existing test used `rm -rf /` as its sample
  and now uses `cargo build`), and `host/tests/forbidden_e2e.rs` with `fixtures/command_agent.py`: a real host and a bot
  in `auto` refuse `rm -rf /`, `Remove-Item -Recurse -Force C:\` and `echo ok && sudo reboot now`, and still approve
  `echo hello`.

## Decisions where there were two options

- **Literal star.** Mercury's `rm -rf /*`, `C:\*` and the generated `?:\*` use `*` as "anything", which refuses
  `rm -rf /tmp/build` and `rd /s /q C:\Users\me\proj\build`. A bracketed `[*]` in a pattern is a literal star, so only the
  root, the home folder and their contents are refused. The cost: a custom command that deletes something under a
  drive root is allowed again (it goes to the card or the bot's setting like any command).
- **Where words are commands.** A command is split at separators and its shell wrappers are opened (three deep); quoted
  words in a message are not commands (`git commit -m "rm -rf /"` runs). A truly obfuscated command (`eval`, base64,
  variables) is not caught: the list is a guard against mistakes, not a sandbox.
- **`?` stays plain in the user's rules**, so no existing rule changes meaning.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | passes |
| `cargo clippy --all-targets -- -D warnings` | passes |
| `cargo test -j 1 --bin codync-host -- --test-threads=1` | 827 passed, 1 failed (the `external` test that used `rm -rf /`); fixed, and the module then passes (29 tests, the new one included) |
| `cargo test --test forbidden_e2e` | passes |

## Not tested

No UI changed. Only an ACP agent and a hook were exercised, both scripted. A real Claude Code or Codex was not run against
the list.
