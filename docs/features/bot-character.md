# What the user writes about how a bot works

Fields of a bot's config that only the person edits. A managing bot has no tool that sets them (a test in
`host/src/chat/manage/tests.rs` fails if one appears), unlike memory, which the bot writes itself.

| Field | What it does | Code |
| --- | --- | --- |
| `verifyCommand` | Run in the bot's workspace after each turn that ended normally. Its exit code decides: 0 is done. Any other code, a start failure or five minutes without finishing makes the turn end as `verification_failed`: the chat shows the last 1,200 characters of its output, a routine run fails and the alert is a failure. | `host/src/agent/bot/verify.rs` |
| `personality` (`soul`, `persona`, `heartbeat`) | Three texts of up to 4,000 characters that join the bot's instructions as a labelled section and reach live sessions through the profile update. They never override permissions. | `host/src/agent/personality.rs` |
| `backgroundModel` | The model routine runs and scheduled check-ins use instead of `model` (and without its options). A check-in on a bot with one runs in a session of its own (`check-in`), because a session's model is fixed when it starts. | `host/src/agent/bot/launch.rs`, `session.rs` |
| `saver` | With a background model, once the bot's (or the fleet's) daily token budget reaches 80 percent, new sessions start on the background model until the next day, and the budget warning says so. A session already running keeps its model, and the context is not reduced: both would need the system prompt to change under a live session. | `host/src/budget.rs` (`saving`), `session.rs` |

Edited in desktop, Android, iOS (not compiled) and the terminal.

## Your week and the Monday recap

`weekSummary {days}` counts the last days from the local transcripts (messages, replies, tool calls, file edits,
approvals, busiest bots; never message text). The usage screens of all four clients show it. On Monday morning
(09:00 to 12:00, this computer's clock) a push tells the same numbers once per ISO week; what was told is kept in
`recap:<week>` before the push, so a restart cannot repeat it. Code: `host/src/store/week.rs`,
`host/src/chat/recap.rs`.
