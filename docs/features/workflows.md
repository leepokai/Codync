# Workflows

A workflow belongs to one computer. It is a graph of triggers, prompts, agents, conditions, approvals and outputs, and the same workflow is a script. The desktop app draws the graph. The iPhone and Android apps edit the same graph as a step list when the shape is an outline, and edit settings only when it is not. The terminal UI uses that list. The host (`host/src/workflows/`) saves the graph, validates it, runs it, and owns schedules and webhooks.

Routines stay per bot (`host/src/routines/`). A workflow can call more than one bot on this computer in one run. The two are not merged and nothing is migrated between them.

The implementation spec is [the design](../design/workflows.md). A script the canvas can draw runs as the graph. Anything else (parallel, pipeline, loops, phases) runs by replaying that script in a child process. A bot can read and rewrite only the workflows it created. Saving or running one waits for the user to allow it. A save changes the next run.

## What a run does

One workflow has at most one open run. Starting another returns `A run is already in progress`. A manual run of a graph that has steps and no trigger starts from the steps with no incoming line. A blank canvas, a script, or a schedule or webhook without a trigger returns `Add a trigger first`.

A manual run is allowed while the workflow is paused. A schedule or webhook does not start a paused workflow. Pausing does not cancel the run that is already going, and it does not bump the graph revision.

An agent step is a turn on that bot, in its own session slot `workflow-{stepId}`, with a notice in the bot's chat. The bot's reply is the step's output. Incoming trigger text is labeled as untrusted event content, not a new instruction. When the workflow was created by a different bot, the prompt says so. A step that repeats more than 20 times fails with `This step repeated too many times`.

An approval step waits until someone accepts or rejects it. Rejecting stops the run. An output step either posts a notice into a bot's chat or sends a done notification. The notification's `userInfo` carries the workflow, run and step ids.

A script replay runs in `codync-host workflow-script`. The parent kills it after two seconds. The child caps its address space, and boa stops a loop, a deep call, or a deep stack. `RegExp` and the string methods that take one are not available. The host's `match()` is the linear matcher. The replay does not hold the workflow gate.

## Who can change a script

A workflow a person creates has no owner bot. The workflows tools do not list, read, save or run it. A workflow a bot creates stores that bot as `owner_bot_id` and starts enabled. `list_workflows` and `read_workflow` return that bot's workflows only. `save_workflow` and `run_workflow` wait on an approval card even when the bot's permission is Auto. Every `agent()` in a bot-saved script names that same bot. Starting a script step checks the bot id again, so `const run = agent; await run("other-bot", ...)` does not start the other bot. Another bot's workflow is reached with `message_bot`, which asks that bot to use its own `run_workflow`.

## Saving

`saveWorkflowGraph` is compare-and-swap on `baseRevision`. A conflict is a successful response (`ok: false`, `conflict: true`) whose `workflow` is the current detail. The editor replaces its graph from that and drops the unsaved edit.

The host keeps the last 50 versions. The first save of a revision is a version; another save within five minutes does not add one.

Deleting a workflow deletes its nodes, edges, versions, triggers, secrets, receipts, runs and steps explicitly. SQLite foreign keys are off.

Deleting a bot clears that bot from workflow nodes and bumps the graph revision, so an open editor conflicts on the next save instead of writing the stale bot back.

## Events

Changes go out as a live `workflow` event (`graph`, `enabled`, `run`, `step`, `deleted`). They are not in the catch-up list and they have no `rev`. Clients that do not know the type ignore it. `hello.workflows` is `true`. Missing that field means the computer is too old to show the entry. This does not raise `minApp`.

## Webhooks

A workflow webhook is one more row in the hook list the computer already publishes to the cloud (`{"t":"hooks"}`). The public URL stays `POST /v1/hooks/{computerId}/{hookId}`. On the computer the path is `POST /hooks/workflows/{hookId}`. A private relay does not serve public webhooks. No new cloud route.

The signature is checked before the workflow gate, and before a paused workflow is distinguished from an unknown one. An unknown hook and a bad signature are both unauthorized. A GitHub `ping` does not start a run. The run stores the parsed event, and the agent sees it as untrusted trigger data. A delivery id is remembered. A body with no delivery id is not stored as a permanent receipt, so the same body can run again.

## Next

Still to walk on the real apps: desktop create, connect, run and history; the same path on the iPhone simulator and, where the SDK is installed, the Android emulator. Still out of this version: a phone canvas, a model node, image nodes, message triggers, output beyond a notification or a chat, a condition that asks a bot, a script that calls another workflow, a token budget, and a worktree per step. Details are in the design.
