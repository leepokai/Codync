# Personal agent: daily and project work

Status: **in progress**, planned 2026-11-13. First product milestone in the [roadmap](../roadmap.md). Built so far:
[bots, projects and threads](bots-projects-threads.md) (on `dev` since 2026-11-25).
This document defines intended behavior and acceptance; it does not describe a shipped project-task model.

## Outcome

A user can keep a personal bot for daily tasks, enter a project for coding work, inspect the result and return to
daily conversation. They should not need a separate bot for every repository merely to preserve work contexts.

The milestone flow is:

1. Start with a daily conversation in the bot's personal workspace.
2. Choose a project and execution computer explicitly.
3. Start a coding task with visible context and permissions.
4. Follow progress, respond to approval requests, cancel or recover if needed, and inspect the result.
5. Return to daily work; revisit the project task later without confusing it with other projects.

## Existing foundation

[Bot workspaces](../features/bot-workspaces.md) already provide a persistent personal workspace and optional project
directories. A bot can use explicit paths for a task without changing its configured directory. Changing its default
project in settings deliberately reconfigures the agent session. These capabilities are the starting point;
they do not by themselves provide the complete interaction described here.

Existing [memory](../features/context-and-memory.md), [routines](../features/routines.md),
[network](../features/codync-network.md) and [cloud computers](codync-cloud.md) remain authoritative for current behavior.

## Intended boundaries

| Concept | Responsibility in the experience |
| --- | --- |
| Personal bot | Identity, user relationship, personal preferences, daily conversations and routines |
| Project | Repository or working directory, project knowledge and applicable project instructions |
| Task | A concrete request, execution context, lifecycle and inspectable result |
| Computer | Where execution and files live; local, remote or cloud |
| Harness/session | Runtime capabilities and conversation context, managed by the host |

These are product concepts, not a commitment to new database entities or wire names. Final schemas and session reuse
rules require a design grounded in current runtime behavior.

- Entering a project task must not silently change bot defaults or the context of unrelated routines.
- Switching visible work must not silently cancel, restart or relocate an active task.
- Relevant personal preferences may carry across tasks; project-specific information needs a defined scope.
- Project files and instructions apply to the intended project; credentials and permissions are not implicitly shared.
- Returning to daily work preserves project history and results, with an explicit way to revisit them.
- Keep the computer and project visible wherever the user starts or approves work. Paths refer to that computer.

## Delivery sequence

### A. Audit and decide

Walk existing daily and project flows on desktop, iPhone, Android and terminal. Record friction and reuse existing
workspace, conversation, task and navigation concepts before proposing new ones.

Write a design covering:

- Entry into project work, return to daily work, and revisiting project tasks.
- Whether and how conversations and ACP sessions are reused, separated or resumed.
- Project identification, moved/missing folders and changing the execution computer.
- Memory scope, retrieval, forgetting and interaction with harness-owned memory.
- Task state, approvals, cancellation, background programs and recovery after host restart.
- What happens when another project is opened while a task is still running.

Exit: the design resolves these decisions and maps each user action and state to host-owned behavior and each client.

### B. Implement one vertical flow

Implement the daily → project → result → daily flow on one computer, with matching behavior across clients.
Keep product logic in the host; clients render state and send intents. Include loading, empty, error, unsupported
capability and permission states. Reuse existing components and adapt navigation to each platform.

Add feature-use, outcome and failure-category events under the [analytics rules](../features/analytics.md#adding-a-feature).
Never include prompts, project paths, repository contents or memory text in analytics.

Exit: the core flow and context-boundary scenarios below pass on the supported clients.

### C. Harden and expand

Validate cancellation, failures, restart and context recovery across the supported agent capability matrix.
Use repeatable memory fixtures to test continuity and isolation. Then extend the same experience to explicit
cross-computer handoff and cloud execution under roadmap stage 2.

## Acceptance scenarios

| Scenario | Required observation |
| --- | --- |
| Daily task | No repository selection is required; the bot uses its personal workspace |
| Enter project A | The user can identify the computer, project and applicable task context before execution |
| Inspect completion | Status and results are available, including relevant changed files or artifacts where supported |
| Return to daily | Daily work resumes without changing defaults or losing the project task |
| Revisit project A | Previous results are accessible and the continuation behavior is explicit |
| Move to project B | Project A's private knowledge and permissions are not silently applied to B |
| Relevant preference | An allowed personal preference remains useful in both daily and project work |
| Forget a fact | Retrieval respects forgetting; harness-memory limitations are made explicit |
| Switch while running | The original task remains discoverable and its lifecycle does not change silently |
| Cancel or crash | State settles visibly; owned processes follow the documented lifecycle; recovery does not duplicate work |
| Missing folder or access | The error names the problem and offers recovery without switching projects silently |
| Host restart | Durable task history survives; resumable and non-resumable work are distinguished |
| Routine during project work | The routine keeps its intended execution context |
| Unsupported harness | The limitation is visible before the user relies on that capability |

Use synthetic data for memory and isolation fixtures. Test process behavior below the UI and walk the full interaction
on the real apps. Apply repository cross-platform validation requirements; record checks that could not run and why.

## Scope and follow-through

This milestone does not add shared ownership, team roles, automatic live-session migration, or another sandbox provider
as a prerequisite. Existing features remain maintained. User-requested parallel tasks and Git worktree automation need
explicit design decisions rather than being assumed by the word "project".

Track implementation through issues using the [maintenance guide](../guides/roadmap-maintenance.md). Update this plan
with linked issues/PRs, acceptance evidence and the release carrying each completed step. Update affected feature docs
and the [user manual](../guides/user-docs.md) in implementation changes, not as though this proposal already shipped.
