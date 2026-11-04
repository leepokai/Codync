# Topic 6: "Always" as a rule, and `actionRules`

## What changed, by file

### "Always allow" as a rule
- `host/src/agent/always_rule.rs` (new): `propose(card)` gives the rule an `allow_always` answer means (the card's command,
  else its title, and its tool kind), `check(rule)` refuses a blank or over-long pattern, a chained one, a non-allow
  decision, and any pattern that would allow a forbidden command (tried as written, with `*` as nothing and as a word),
  and `added` appends without duplicates.
- `host/src/api/bots.rs`: `proposeRule {entryId}` returns `{rule}` or `{rule: null, refusal, reason}`;
  `respondPermission` accepts `rule`, refused unless the picked option is `allow_always`, saved through `update_bot` before
  the answer goes out.
- Desktop: the permission card asks for the proposal, shows the pattern in an editable box ("Keep rule and allow"), and says
  why there is no rule (with "Allow once") when the host refuses one. Terminal: `a`/the option opens a small sheet to edit
  the pattern (`↵` keeps and allows, `o` allows once, `esc` cancels). Android (a phone): "Always allow" is not offered when
  "Allow once" is. The notch island is unchanged: it answers "always" with no Codync rule.

### `actionRules`
- `host/src/agent/approval_rules.rs`: `quick_rules(policies)` turns the classes that have ACP tool kinds into one `*` rule
  per kind; `evaluate` takes the person's rules and the quick rules and says in `Outcome::Rule` whether the deciding rule was
  quick (`quick`, `kind`). `QUICK_KINDS` and `QUICK_UNENFORCED` are the table and the reasons.
- `BotConfig.action_rules_on` (new, default false): nothing is enforced until it is on. The real approval and
  `testApprovalRule` (which also takes unsaved `actionRules`/`actionRulesOn`) use the same code and also return
  `quickNotEnforced`.
- Clients: desktop and Android show the switch and the three classes inside the approval rules editor; the terminal's rules
  sheet has `Q` and `1 2 3`. The tester words a quick match as "by the quick rule for shell commands".
- Docs: `docs/features/approvals.md` ("Always allow" as a rule; Quick rules by kind of action).

## Class by class

| Class | Becomes | Why |
|---|---|---|
| files | read, search, edit, delete, move | ACP tool kinds for file work |
| shell | execute | ACP kind for commands |
| browse | fetch | ACP kind for web access |
| browser, apps, bots, routines | nothing | MCP tools arrive as kind `other` with the tool in the title: a rule on `other` would hit every MCP tool. The connectors' tool policies govern them. |

## Decisions where there were two options

- **Switch, not a migration.** Role presets already store `actionRules` on bots, so enforcing them silently would change
  behavior. `action_rules_on` is the "user turns it on" the task asks for.
- **Quick rules after the person's own rules**, so an explicit rule always beats a coarse one.
- **The rule is saved before the answer goes out.** If the answer then fails, the person still asked for a standing allow;
  the opposite order could answer "always" with no rule kept.
- **The host checks the rule, not who asks.** "Not on the phone" is enforced in the client (Android hides the option); the
  host cannot tell a phone from a desktop on the same channel.
- **iOS is out of scope** by the user's instruction.

## Tests

`always_rule` (propose, title fallback, chained and forbidden refusals, edited pattern, no duplicates);
`approval_rules` (one test per enforceable class and policy, only its own kinds, own rule wins, chained allow, the other four
classes add nothing); `host/tests/approval_rules_e2e.rs` (keep a widened rule and see the next request approved without a
card; refusals of a non-always option, a chained pattern and a forbidden one; quick rules off then on, own rule over quick,
unsaved quick settings tested); the terminal sheets; Android and desktop helpers.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test -j 1 --bin codync-host -- --test-threads=1`, and the e2e tests `approval_rules_e2e`, `forbidden_e2e`, `pause_e2e` | 851 passed, 0 failed, 1 ignored; the e2e tests pass (4, 2 and 2) |
| Desktop `npm run typecheck`, `npm test` | clean; 389 passed |
| Android `assembleDebug testDebugUnitTest :core:test lintDebug` | passes (exit 0, BUILD SUCCESSFUL in 4m 37s) |

## Not tested

No screen was seen: the desktop card and the Android rows were compiled and their logic tested. No real agent offered
`allow_always` against this host.
