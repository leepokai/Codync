# Roadmap

Status: **proposed**, 2026-11-03. Four long-running tracks, in the order the owner listed them. Each track links to the
doc that holds its current behavior and its detailed plan. Shipped work stays in those docs, not here; when a track
finishes a step, update its plan doc in the same change and trim the item below.

Rules that apply to every track: [CLAUDE.md](../CLAUDE.md) (version bumps, e2e, cross-platform UI), [docs/plans/](plans/)
(detailed plans), [docs/archive/](archive/README.md) (finished plans).

## 1. Cloud plan

Goal: users who want their bots on a cloud computer bring their own sandbox, with no charge from Codync and without
changing how the host owns the logic.

Where it stands:

- The Codync cloud already carries the encrypted relay (`cloud/`, ComputerRelay DO), accounts, public routine
  webhooks and the cloud assistant connector (`mcp.codync.dev`, [cloud assistants](features/cloud-assistants.md)).
- A private relay is supported and has no accounts ([relay reference](reference/remote-relay.md)).
- The product direction (owner, 2026-11-06): Codync charges nothing, and users bring their own sandbox and API key.
  Memoh (self-hosted per-bot cloud computers) is a possible option; its role is not decided yet.
- `feat/cloud-boat` has the cloud computer flow on Boat, so far with one Codync-held key.
- `cloud/` is deployed by hand, never by CI ([deployment](guides/environments-and-deployment.md#releasing-cloud-changes)).

Next steps:

1. Decide how users bring a sandbox: which providers come first and where their API key lives. Write the decision in
   `docs/design/` before building.
2. Prove one end-to-end path: sign in, start a cloud computer on the user's own sandbox, pair a phone, run a bot, read
   the reply.

Done when: a new user can bring a sandbox, get a working cloud computer, and use it from the iPhone and desktop apps with
no local computer, and the relay docs describe it.

## 2. codync-network: long-term upkeep and stable bot-to-bot exchange

Goal: computers on several networks keep talking as the set of agent types grows, and bot-to-bot messages behave the
same whatever agent is on each end.

Where it stands: [codync-network](features/codync-network.md), [what's left](plans/codync-network-next.md), and
[bot collaboration](features/bot-collaboration.md). Still open there: Codync-run relays (waiting for the owner to
rent servers), the account networks, and the screen and rules items in the plan.

Next steps:

1. Close the open packages in `plans/codync-network-next.md` in build order; move finished ones into the feature doc.
2. Conformance tests for bot-to-bot exchange: one fixture per agent type (Claude, Codex, and each ACP registry agent
   that can run a bot), checking the same message flow: request, reply, timeout, refusal, and a routine that a bot may
   not run for another bot.
3. Keep `host/crates/network` at its 0.x version rule: every release that changes it bumps `codync-network`.
4. Upkeep: each release, run the conformance set against the current agent versions and record failures in
   [upstream agent bugs](upstream-agents/README.md) when they come from an agent.

Done when: the conformance set passes on every supported agent type across two computers, on the same network, in CI
or in a documented manual run, and the network docs list no open "Beta" gaps.

## 3. Memory (codync-mem): benchmarking and optimization

Goal: measure how well bot memory works, then improve it with numbers, not guesses.

Where it stands: [context and memory](features/context-and-memory.md) and the design in
[bot memory](design/bot-memory.md). Bot memory is files (`MEMORY.md` and notes) that the bot edits; dreaming tidies them
into topic notes. The open choice is between a harness's own memory and `codync-mem`
([harness memory](plans/harness-memory.md)).

Next steps:

1. Write a benchmark set: a fixed script of conversations per bot, with questions whose answers depend on earlier
   sessions. Keep it in `host/crates/memory` tests or a fixtures folder; no real user data.
2. Metrics: recall (right fact returned), precision (no wrong or stale fact), tokens added to each turn, time to
   first token, and notes size after N dreams.
3. Run it on the current memory, then on the harness-memory option, and publish the table in `docs/reference/`.
4. Optimize the worst metric first. Keep the rule that memory is never full: dreaming moves detail into topic notes.
5. Use the result to decide the harness-vs-`codync-mem` question, and record the decision in the harness-memory plan.

Done when: the benchmark runs from one command, its results are in the repo, and the default memory choice is backed
by those results.

## 4. ACP coordination and long-term maintenance

Goal: every agent that Codync drives over ACP keeps working as adapters change, and the patches around them shrink
over time.

Where it stands: the host drives agents over ACP (`host/src/agent/acp.rs`, `host/src/agent/bot/`). Each agent bug,
patched in the host or not, has a row on its agent's page in [upstream agent bugs](upstream-agents/README.md) (last
checked 2026-11-02). Open upstream items include codex-acp#647 (429s and other errors sent as reply text) and the JetBrains AIR opt-in that works around it.

Next steps:

1. Make the adapter check a routine: on each release, check every registry adapter version against the workaround
   table; delete rows whose upstream fix has shipped, and add rows for new bugs in the same change.
2. Track upstream issues in one list (owner, status, which workaround it unblocks) and re-check it on the same cadence.
3. Once an adapter's rows are mostly gone, reconsider its place in the registry, or whether another implementation is
   worth it. Record the decision in the workarounds doc.
4. Keep the ACP parser tolerant (updates stay `serde_json::Value`), and add a test for each new adapter variant seen
   in the field.

Done when: the workaround table is checked on every release, each row names an upstream issue or a delete condition,
and no adapter is patched in code without a row.

## Order and dependencies

- Track 1 (cloud) sets the cloud option; it does not block the others.
- Track 2 needs the adapter list from track 4, so run track 4's routine check first.
- Track 3 feeds track 1 if cloud computers need a memory default.

## Open questions

- Which sandbox providers can users bring first (Boat, Memoh or others)?
- Does the memory benchmark run in CI, or as a manual run on a dev computer?
- Who owns the adapter check each release (a routine, or the release checklist)?
