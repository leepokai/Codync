# Workflows

A workflow belongs to one computer. It is a graph of triggers, prompts, assistants, conditions, approvals and outputs. The desktop app draws the graph. The iPhone and Android apps edit the same graph as a step list when the shape is an outline, and edit settings only when it is not. The terminal UI uses that list. The host (`host/src/workflows/`) saves the graph, validates it, runs it, and owns schedules and webhooks.

Routines stay per bot (`host/src/routines/`). A workflow can call more than one bot on this computer in one run. The two are not merged and nothing is migrated between them.

The implementation spec is [the design](../design/workflows.md). The same workflow is also a script. A script the canvas can draw runs as the graph. Anything else (parallel, pipeline, loops, phases) runs by replaying that script. A bot on this computer can read and rewrite it with the built-in workflows tools. A save changes the next run.

## What a run does

One workflow has at most one open run. Starting another returns `A run is already in progress`. A graph with no trigger can be saved; starting it returns `Add a trigger first`.

A manual run is allowed while the workflow is paused. A schedule or webhook does not start a paused workflow. Pausing does not cancel the run that is already going, and it does not bump the graph revision.

An assistant step is a turn on that bot, in its own session slot `workflow-{stepId}`, with a notice in the bot's chat. The bot's reply is the step's output. A step that repeats more than 20 times fails with `This step repeated too many times`.

An approval step waits until someone accepts or rejects it. Rejecting stops the run. An output step either posts a notice into a bot's chat or sends a done notification. The notification's `userInfo` carries the workflow, run and step ids.

## Saving

`saveWorkflowGraph` is compare-and-swap on `baseRevision`. A conflict is a successful response (`ok: false`, `conflict: true`) whose `workflow` is the current detail. The editor replaces its graph from that and drops the unsaved edit.

The host keeps the last 50 versions. The first save of a revision is a version; another save within five minutes does not add one.

Deleting a workflow deletes its nodes, edges, versions, triggers, secrets, receipts, runs and steps explicitly. SQLite foreign keys are off.

Deleting a bot clears that bot from workflow nodes and bumps the graph revision, so an open editor conflicts on the next save instead of writing the stale bot back.

## Events

Changes go out as a live `workflow` event (`graph`, `enabled`, `run`, `step`, `deleted`). They are not in the catch-up list and they have no `rev`. Clients that do not know the type ignore it. `hello.workflows` is `true`. Missing that field means the computer is too old to show the entry. This does not raise `minApp`.

## Webhooks

A workflow webhook is one more row in the hook list the computer already publishes to the cloud (`{"t":"hooks"}`). The public URL stays `POST /v1/hooks/{computerId}/{hookId}`. On the computer the path is `POST /hooks/workflows/{hookId}`. A private relay does not serve public webhooks. No new cloud route.
