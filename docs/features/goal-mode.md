# Conversation goals

A person can set a goal in the desktop conversation details. Starting it queues the
first step immediately; its banner shows the goal, the steps used and a stop button.
Once stopped, Clear goal removes the banner and returns to an ordinary conversation.
The host owns continuation. Defaults are 20 turns and 60 minutes; accepted limits
are 1–200 turns and 1–1440 minutes. A new goal replaces the previous one.

The host stops automatically on an affirmative closing completion marker, a question
or NEEDS_USER marker, an approval card, an agent error, a user message (including
voice), or a limit. Queued steps carry a generation id so stopping or replacing a
goal invalidates them. Ordinary call openings, rooms, routines and delegated turns
cannot drive the loop. The agent is told to close with GOAL_COMPLETE only on success.

Before each step, the host checks its existing subscription usage protection
(over 75% of a reported plan window). Providers without usage readings still have
turn and time limits; this is not a token or currency budget. Completion and
free-text questions are heuristic; structured approval cards always stop continuation.

API: setGoal takes botId, goal, optional maxTurns and maxMinutes. Empty goal clears
it; malformed limits, unknown bots and groups are rejected. stopGoal stops continuation.
Bot snapshots and events include goal (null when unset). Old hosts omit this field,
so desktop hides the entry there. Changes are additive; no compatibility floor changes.

This first increment exposes controls on desktop only, as requested for #411.
Phone and terminal clients show the host's notices and can stop continuation by
sending a message or stopping the bot. Dedicated controls and device validation
remain outside this increment. Desktop reads the existing shared Chinese dictionaries.

The feature is opt-in. No background goal is resumed on host restart.
Running steps are cancelled at the time limit; unfinished session setup is cleaned
up before the next turn. Stop goal also cancels its running step. A user message
stops further automatic steps and follows the usual queued-message behavior.
Telemetry records goal_started and goal_stopped with the stop category only.
The release coordinator applies a version bump before merging into main.
