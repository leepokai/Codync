# Topic 9: questions and plans from external sessions

## What changed

- `host/src/external/install.rs`: a second `PreToolUse` group (`matcher` `AskUserQuestion|ExitPlanMode`, command
  `... hook --ask`, 120 s). `installed()` needs it; the person's own hooks stay.
- `host/src/cli/hook.rs`, `main.rs`: `--ask`; only Claude Code (no `--agent`) and only those two tools wait; the envelope
  carries `ask`.
- `host/src/external/question.rs` (new): turns `AskUserQuestion` into one card per question and `ExitPlanMode` into an
  Approve/Reject card; `permission.rs` shares the wait (`wait_card`) and now validates that an answer is one of the card's
  options. `Decision::Pick(index)`.
- `host/src/external/reply.rs`: `pre_tool` prints the `PreToolUse` hook output (`allow` + `updatedInput.answers`, or
  `deny` + reason) only from a verdict the host made after a person's choice.
- Clients: the cards are Permission entries, so desktop (and the island), Android and the terminal already show the
  options as buttons; the headlines say "Asks you a question" / "Wants your approval of a plan" (desktop, Android).
  iOS is out of scope by instruction.

## Tests

`external::tests` (two questions answered by label, an unknown option ignored, a late tap is a no-op, cancel leaves the
terminal to ask, plan rejection, the plain hook and other tools never ask), `question` unit tests (shape, limits),
`reply` (nothing allowed without a choice), `hook::prepare` (who asks), `install` (the ask group).

## Not tested / limits

- No real Claude Code session ran: the `updatedInput.answers` contract comes from its hooks documentation.
- Approving a plan by `allow` is not documented by Claude Code; rejecting is. If Claude ignores the allow, the terminal
  shows its own plan prompt.
- Multi-select questions are left to the terminal. No screen was seen.
