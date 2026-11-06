# Topic 14: proposed skills inbox

**Written, not compiled, not run** (the user asked for the gates to wait for one pass at the end).

## What changed

- `host/src/chat/skill_proposals.rs` (+ `skill_proposals/tests.rs`): the procedure test (`tool_keys`, `worth_a_skill`),
  fingerprint, drafting with `chat/oneshot.rs`, the inbox (list, edit, approve through `chat::skills::call`, dismiss) and its
  rules (budget, one draft an hour, no repeats). `store/entries.rs`: `tools_in_turn`. `agent/bot/turn.rs`: a turn that ends
  `end_turn` hands its tools to `consider`. `api/bots.rs`: the four methods.
- Desktop: `shared/skill-proposals.ts` (+ test), `SkillProposalsCard.tsx` under the Memory tab. Android:
  `core/SkillProposal.kt` (+ test), `SkillProposalsCard.kt` under the Memory tab. Terminal: `tui/app/proposals.rs` and view.

## Decisions

- Stored in the key-value store, not a new SQL table: a short list per bot, nothing to join.
- "Paused by budget" is read as the budget's warning level (`Budgets::saving`), the cautious reading.
- The count shows in the inbox title, not as a badge on the bot's row: the roster does not know about proposals.

## Not tested

Nothing compiled or run. No real agent drafted a skill (the draft prompt and the parse of its reply are tested with text
only). No screen seen. iOS not done.
