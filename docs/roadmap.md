# Roadmap

Direction agreed with the owner: **personal agent first, multiplayer AI later**, 2026-11-13.
The stages define product priorities; the four technical tracks support them continuously.
These are goals and acceptance criteria, not claims of shipped behavior or commitments to release dates.

Detailed plans live in [plans/](plans/); current behavior lives in [features/](features/).
Move completed plans to [archive/](archive/README.md) and update corresponding feature docs.

## Product stages

### 1. Personal agent: daily tasks and project work

Make one person's everyday agent useful and dependable. The immediate milestone is:

**Daily conversation → select a project → execute a coding task → inspect the result → return to daily conversation.**

- Bot-based work retains the bot's identity, personal preferences, conversations and routines.
- Project-based work makes the computer, repository, working directory, task context, execution state and results explicit.
- Personal preferences can carry across tasks; project knowledge, permissions and execution context have clear boundaries.
- Entering a project task must not silently reconfigure unrelated conversations or routines.
- Starting, cancelling, recovering and understanding a failed task are part of the experience.

Done when: a user completes this flow without recreating a bot or accidentally mixing project contexts, with consistent
behavior on desktop, iPhone, Android and terminal. See the [personal agent integration plan](plans/personal-agent-experience.md).

### 2. Personal agent across computers

Make the same person's local computers, remote computers and cloud sandboxes understandable execution destinations.
Build on the existing network and cloud paths.

- Make destination and handoff explicit, including the context and access the receiving agent gets.
- Keep status and results understandable through disconnection, reconnect, timeout and remote failure.
- Validate communication between supported coding agents before expanding the agent list.
- Make cloud lifecycle management support the same personal workflows.

Done when: a user sends a task to another computer they own and can track where it runs, its result and failure reason.
Validate on two real hosts. This does not promise transparent migration of a live ACP session or automatic sharing of
all memory and credentials.

### 3. Multiplayer AI

After personal workflows and cross-computer boundaries are dependable, define shared agents and projects,
membership, permissions, collaborative tasks and attributable activity.

Entry condition: stages 1 and 2 pass their acceptance scenarios, including context boundaries and recovery.
Detailed multiplayer behavior and acceptance criteria need a separate design before implementation. Existing
collaboration remains maintained throughout; this ordering governs new product expansion.

## Four continuous technical tracks

### 1. ACP compatibility and agent process management

Goal: supported agents execute tasks reliably throughout their lifecycle.

- Cover launch, authentication failure, session resume, cancellation, crash recovery, process cleanup and host restart.
- Define ownership and lifecycle for task processes and background programs so cancellation and recovery are predictable.
- Maintain common conformance scenarios with capability-specific expectations for each supported adapter/version.
- Record upstream defects, workarounds and removal conditions; recheck when adapters change and before release.

Acceptance: supported lifecycle scenarios pass, unsupported capabilities are explicit, failures reach the user,
and every workaround has a documented reason and removal condition.

References: [conversation runtime](architecture/conversation-runtime.md),
[upstream agent bugs](upstream-agents/README.md), [dev servers](features/dev-servers.md).

### 2. Agent memory

Goal: useful continuity with understandable personal, bot and project boundaries.

- Define what carries from daily work to a project, what stays scoped to that project, and how users inspect or forget it.
- Resolve Codync-memory versus harness-memory choices with explicit ownership and tradeoffs.
- Benchmark recall, wrong or stale recall, tokens added, latency and growth after repeated consolidation.
- Include cross-project leakage and forgotten information returning in acceptance scenarios.

Acceptance: repeatable fixtures support memory decisions; switching work contexts preserves relevant preferences
without exposing unrelated project knowledge. Storage and harness details remain in their authoritative docs.

References: [context and memory](features/context-and-memory.md), [bot memory](design/bot-memory.md),
[harness memory](plans/harness-memory.md).

### 3. Agent network and coding agent support

Goal: reliable exchange between agents on the user's computers, with more agents added when they can be maintained.

- Prioritize open network work that blocks personal cross-computer workflows.
- Test request, reply, timeout, refusal, reconnect and prohibited operations across supported agent combinations.
- Distinguish transport delivery from task acceptance and completion; document retry and duplicate behavior.
- Add agents based on user needs, demonstrated compatibility and ongoing maintenance cost.

Acceptance: documented supported combinations pass two-host scenarios and expose actionable failure states.

References: [codync-network](features/codync-network.md), [remaining network work](plans/codync-network-next.md),
[bot collaboration](features/bot-collaboration.md).

### 4. Agent cloud sandbox management

Goal: the personal agent works reliably even when the user has no local computer running.

- Improve the existing bring-your-own-cloud lifecycle: provisioning, agent sign-in, sleep/wake, updates and recovery.
- Make persistence, credentials, access boundaries and provider failures understandable.
- Validate unattended routines and message-triggered wake as part of the personal experience.
- Expand providers after supported paths work reliably and a concrete user need justifies another adapter.

Acceptance: a user provisions a cloud computer, signs into an agent, completes a task, sleeps and wakes it, and
recovers from a failed operation without losing durable work.

Reference: [Codync Cloud](plans/codync-cloud.md), which records the existing BYOC implementation, release history and
remaining delivery work. Codync remains free; users bring their own provider account. Cloud changes follow the
[manual deployment rules](guides/environments-and-deployment.md#releasing-cloud-changes).

## Maintenance and execution

Issue and PR maintenance runs throughout every stage, following the
[roadmap maintenance guide](guides/roadmap-maintenance.md) and existing [issue and PR review](guides/issue-pr-review.md).

Focus on one product milestone at a time. Fix serious reliability regressions as they arise; choose technical work
by the milestone it enables. Stage 1 starts with ACP/process reliability and memory/context boundaries. Network and
cloud expand that experience into stage 2. Product logic remains in the host under the
[ownership rules](architecture/overview.md#host-owns-the-logic).

Review this roadmap at milestone completion and when user evidence changes priorities. Do not infer completion from
a merged PR alone: record release and acceptance evidence in the detailed plan.
