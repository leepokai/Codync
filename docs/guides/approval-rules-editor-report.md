# Topic 5: approval rules editor

## What changed, by file

- `host/src/agent/approval_rules.rs`: `evaluate(rules, permission, kind, command, title) -> Evaluation` is now the only
  place that decides a permission request: forbidden command (topic 3), the first matching rule (an `allow` rule is
  passed over for a chained command), or the bot's setting. `Outcome` and `Evaluation` serialize for the API;
  `is_chained` is public. The old `decide` stays only for the existing tests.
- `host/src/agent/bot/updates.rs`: `on_permission` calls `evaluate` instead of repeating the steps, so a real request and
  the tester cannot disagree.
- `host/src/api/bots.rs`: `testApprovalRule {botId, command, kind?, rules?}` returns `{evaluation}`. `rules` tests an
  unsaved list (validated like a saved one), so the editors never copy the logic.
- Desktop: `shared/approval-rules.ts` (types, wording, move, clean, limits; tested), `views/bots/ApprovalRulesEditor.tsx`
  (+ css) in the bot settings under Permissions, `store.testApprovalRule`, and `save` drops blank rules.
- Android: `core/ApprovalRules.kt` (rule, evaluation, wording, move, clean; tested), `BotApprovalRulesRows.kt` under
  Permissions, `AppStore.testApprovalRule`, and `BotDraft` carries and cleans `approvalRules`.
- Terminal: `app/rules.rs` and `view/rules.rs`, an "Approval rules of this bot…" sheet in the palette: `a` add, `↵`
  edit, `d` delete, `K`/`J` move, `m` decision, `t` tool kind, `T` try a command, `^s` save.
- Tests: `evaluate` (rule, forbidden before any rule, chained allow passed over, request judged by title), desktop and
  Android helpers, the terminal sheet (load, move, cycle, body, drawing through a `TestBackend`), and
  `host/tests/approval_rules_e2e.rs`.

## Acceptance

"Create, test and delete a rule changes the behavior of a real approval": the e2e starts a real host with a scripted agent
and a bot on `ask`. With no rule the tester says the setting asks. After `updateBot` adds `echo *` → allow, the tester says
rule 1 allows it, and a real request is approved with no card. The chained `echo hi && ls` is not allowed by the rule (and
the tester names the passed-over rule). An unsaved list can be tested. After deleting the rules the same request waits for
a card, and an allow-everything rule still does not let `rm -rf /` through.

## Decisions where there were two options

- **One function, not two.** The tester calls the same `evaluate` as the real approval. The alternative, a copy of the
  matching in each client, would drift.
- **Rules travel with `updateBot`** (the contract already existed); no new save method.
- **The terminal sheet lives in the palette**, not as a row of the settings form, because the form is a flat list of
  single-line fields and a list editor needs its own keys.
- **iOS is out of scope** by the user's instruction.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test -j 1 --bin codync-host -- --test-threads=1`, `--test approval_rules_e2e` | 840 passed, 0 failed, 1 ignored; the e2e passes (one clippy style fix in the new terminal test came after the full run, then fmt, clippy and the test were run again) |
| Desktop `npm run typecheck`, `npm test` | clean; 389 passed |
| Android `assembleDebug testDebugUnitTest :core:test lintDebug` | passes (exit 0, BUILD SUCCESSFUL in 5m 29s) |

## Not tested

The desktop and Android editors were compiled and their logic tested, not seen on screen. The terminal sheet was drawn only
through a test backend. No real agent asked for a permission against these rules.
