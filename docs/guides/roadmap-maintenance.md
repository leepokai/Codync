# Roadmap, issue and PR maintenance

Status: planning practice, 2026-11-13. This guide makes the [roadmap](../roadmap.md) actionable.
It supplements existing [issue and PR review](issue-pr-review.md); it does not replace its security,
approval, CI, review or merge requirements. The cadence here is a working practice, not configured automation.

## Priorities

Triage by user impact first, then by what blocks the active product milestone:

1. Security incidents, data loss or broken access boundaries.
2. Inability to start, cancel, resume or recover work; serious supported-agent regressions.
3. Friction or missing behavior in the current milestone's end-to-end flow.
4. Other compatibility and reliability work in existing supported paths.
5. New agents, providers and future-stage features.

Severity can move any item up. Association with a technical track is not itself a reason to prioritize an item.
Focus on one product milestone at a time and keep the rest of the backlog ordered. Continue necessary
maintenance of existing network, cloud and collaboration features.

## Issue record

For an accepted issue, record these fields in its body or existing project tooling; new GitHub labels are optional:

- User problem, impact and expected behavior.
- Kind: bug, feature, compatibility or maintenance.
- Product stage/milestone and primary technical track, where applicable.
- Reproduction and environment, including relevant app, host, harness and adapter versions; redact secrets and content.
- Acceptance criteria and validation needed, including affected clients and computer boundaries.
- Owner, next action, blockers and related issues or PRs. Use "unassigned" when nobody has accepted ownership.

Link duplicate reports to one canonical issue. If waiting on upstream, link the upstream item and relevant
[agent defect record](../upstream-agents/README.md), state user impact and workaround, and specify the condition
for retesting or removing that workaround.

Suggested issue lifecycle: **triage → ready → in progress → in review → closed**.
Blocked work states its dependency and next recheck trigger. Invalid, duplicate and unreproduced reports follow
the existing review process's outcomes rather than being forced through this lifecycle.

## PR record and completion

Every PR explains the user-visible change, links its issue or milestone, and records validation, known limitations and
affected documentation. Apply existing requirements for cross-platform changes, versions, phone release notes,
screenshots and review gates. Keep changes small enough to review as one logical change.

A fixed issue is closed when its fix is merged into `dev`, as the [review guide](issue-pr-review.md) says. A roadmap
milestone also needs the release and recorded acceptance evidence. Link that evidence
from its detailed plan; never mark unavailable validation as passed.

## Cadence and ownership

- **Weekly triage:** the maintainer or a designated owner reviews new issues, stalled PRs, regressions, duplicates and
  upstream blockers; assigns next actions and updates the active milestone. Contributors are not assigned without agreement.
- **On an agent/adapter change:** rerun affected compatibility scenarios, check upstream fixes and update defect
  records. Remove workarounds only when supported versions no longer need them.
- **Before each release:** check supported lifecycle scenarios and known regressions, recheck workaround records,
  and verify the release includes required docs and compatibility changes.
- **After release:** link the shipped version and record failures that remain.
- **At milestone completion:** review acceptance results, move shipped behavior into feature docs, archive completed
  plans and select the next milestone against the personal-first roadmap.

If maintenance is automated later, define its owner, permissions, schedule and failure reporting separately.
No routine described here authorizes skipping review gates or publishing messages to external contributors.
