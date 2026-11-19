# Group workspaces

Status: phase 1 ships in 2.51.0 (group field + validation + `hello` capability + session
`additionalDirectories` + room-prompt rules + all clients' editors). Phase 2 items (team
snapshot, `manage` MCP group surfaces) remain planned, not built. The writeup below is the
design both phases implement against. Naming follows
[groups-and-replies](groups-and-replies.md): a *group* is what users call a team.

As shipped, the analytics are: `bot_updated` gains `workspace` (`set`/`changed`/`cleared`,
never the path); folder-missing at ask time reports `group_workspace_unavailable` once per
breakage (`host/src/chat/group/rooms.rs` latches it).

## Why

A group chat is several bots and the user in one conversation, but the group itself has *no
agent, folder or harness of its own* (`host/src/chat/group.rs`). Each member answers from its
**own** session and folder today. Users who want a team on one project must point every
member at the same folder by hand, and even then there is no coordination —
[bot-workspaces](bot-workspaces.md) states it plainly: "Separate workspaces do not prevent
conflicts when multiple bots edit the same external project."

Goal: one place to give a team a shared folder, and host-owned rules so the members actually
work together in it without overwriting each other.

Non-goals: no distributed file locks, no automatic merging of members' edits, no per-turn
cwd switching, no migration of existing groups.

## Decisions

### The group gains an optional `workspace`

Members are on the same computer (groups are local per roster rules), so a plain absolute
path works for everyone.

- It is a **separate field, not `bot.cwd`**. A group's `cwd` stays empty forever. Reusing
  `cwd` would flip its meaning (today: empty = auto-allocated bot folder), and — fatally —
  an older client round-tripping a group sends `cwd: ""`, which would silently opt the group
  into auto-allocation. A new field is ignored by old clients on both read and write.
- Shape: `workspace: Option<String>` on `BotConfig`
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`). `None`: no shared folder —
  today's behavior, bit for bit. `Some(path)`: an existing absolute folder, validated at save.
- Same validation class as a bot's folder (`host/src/hub/validate.rs`): a folder that is set or
  changed must exist and be a directory. A path that stops existing later (renamed/removed) is
  not re-validated until the folder itself changes, so renaming the group or changing members
  still works; the room and its members then read it as missing (`chat::group::workspace`)
  until the user fixes it.
- **No host-allocated group folder in phase 1.** *Own folder* for a team (host-allocated
  under the data dir, with versioning) is a follow-up (see *Open questions*); phase 1 is
  explicit project folders only.
- Clearing (`null`) keeps the folder on disk. Deleting the group leaves the folder alone.

### Access, not relocation: members keep their own session

A member's history is unified across its chats and its session launches with its configured
`cwd` (`host/src/agent/bot/session.rs`). Changing that per-room would fork the member's
context. Instead:

- **Permission envelope**: the group's workspace joins the member's ACP `additionalDirectories`
  in `session_params` — the exact mechanism memory folders already use
  (`host/src/agent/bot/mod.rs`: "`additionalDirectories`: folders outside `cwd` the agent may
  work in"). A member in several groups lists the union of their workspaces. Agents without
  the capability degrade gracefully: cwd is an execution default, not an access boundary
  (`host/src/agent/workspace.rs`), so the path still works through instructions alone.
- **Steering**: `member_prompt` (`host/src/chat/group.rs`) gains a shared-folder section,
  built once per ask, only when the group has a workspace. This is also where the notes below
  land.

### Coordination is rules + visibility, not locking

Phase 1 prompt section (built by the host, same for every member, kept terse for prompt-cache
friendliness):

```
The team shares the folder {path}. Keep joint output there (your own folder is for work
nobody else needs). It changes between your turns: look before you write, never overwrite
work you have not read, and prefer small scoped edits. Before substantially rewriting a file
a teammate is actively working on, ask in the room. If it is a repository, commit early and
never merge or rebase for someone else.
```

Phase 2 adds **team snapshot**: when the workspace is a git repository, the host appends a
few lines — latest commit, each member's in-flight Codync worktrees (branch `codync/…`,
path; data already tracked in `chat/tasks/worktree.rs` and already exposed per-bot as
`work[].worktree`), and uncommitted-change counts of the main checkout. The same data renders
in the group header on the apps. Task worktrees continue to isolate concurrent tasks per bot
in the shared repo by default; the snapshot makes them visible to teammates.

Explicitly out of scope: file locks, edit queues, automatic merge/rebase. The existing bot
collaboration rule stands ("Coordinate file ownership before asking for edits in a shared
project", `chat/team.rs` `INSTRUCTIONS`).

## Host changes (phase 1)

- `host/src/store/model.rs`: `pub workspace: Option<String>` on `BotConfig` (groups only), doc
  comments updated (`cwd` stays "empty for a group").
- `host/src/hub/validate.rs`: for groups, `workspace` set ⇒ absolute path that is a
  directory; agent bots must not receive it (`"workspace is a group option; bots use cwd"`).
  `agent::workspace::prepare` still skips groups.
- `host/src/hub/bots.rs`: `create_bot` / `update_bot` flow unchanged otherwise (schema-merge
  via `patch::patched` picks the field up automatically).
- `host/src/agent/bot/session.rs` `session_params`: append the member's group workspaces to
  `additionalDirectories` when the agent advertises the capability (memory folders first,
  then group workspaces).
- `host/src/chat/group.rs` `member_prompt`: inject the section above when set; include
  "folder is currently unavailable" when the path no longer exists at ask time.
- Capability + sync: the `hello` advertises a `groupWorkspace` capability; the bot payload
  carries `workspace` on groups.
- Analytics (`docs/features/analytics.md`): `bot_updated` reports the change (`fields` holds
  `workspace`, plus `workspace`: `set`/`changed`/`cleared`); a folder missing at ask time
  reports `group_workspace_unavailable`, once per breakage.

Phase 2 (planned, not phase 1): team snapshot in prompt + group header; `manage` MCP group
surfaces (`chat/manage.rs` today filters groups out of every operation — add
`create_group`/`update_group`/`list` with the same `approve` card flow so team-managing bots
can set a workspace under user approval).

## API (additive on existing methods)

| Method | Body | Notes |
|---|---|---|
| `createBot` | `{kind:"group", name, description?, members, workspace?}` | `workspace` optional; set ⇒ validated folder. |
| `updateBot` | `{id, workspace?}` | String: set/change. `null`: clear. Omitted: unchanged. |
| Bot payload | `workspace?: string` | Present on groups only. |
| `hello` | capability flag | Clients gate the workspace UI on it. |

Compatibility (see [compatibility](../reference/compatibility.md)): new-app/old-host —
`updateBot` patches merge silently on old hosts (`host/src/hub/patch.rs`), so the write would
drop without an error; the capability flag prevents the UI from offering the option at all.
Old-app/new-host — decoders ignore the extra payload key. Additive ⇒ no `minApp`/`minHost`
raise.

## Clients (all four, same concept)

Same rule book as every cross-platform change: one concept, platform-adapted UI.

- **iPhone / Android / desktop**: group editor gains a Workspace row (None, or the folder's
  name) reusing the host folder picker (`selectWorkspace` / `listDirs`, which already records
  recent selections and is local-computer scoped). Group row/header shows the folder when set.
- **Terminal**: group form gains a workspace line; the terminal folder picker (Ctrl+R recent,
  Ctrl+B browse, Ctrl+L path) handles selection.
- Errors (vanished folder, older host) render as the clients' existing inline error states.

## Testing

- Host unit tests: validation (non-absolute, missing folder, workspace on an agent bot),
  patch round-trip (`null` clears, omission keeps), prompt injection present/absent/
  unavailable, `additionalDirectories` union across multiple groups, capability fallback.
- E2E (minimal = this Mac's desktop sandbox via Playwright, headless): create a group → set a
  project folder through the picker → reopen the editor after a reload (the host kept it) →
  send a message and assert a member's turn prompt names the shared folder
  (`apps/desktop/e2e/group-workspace.spec.ts`, scripted agent). iOS/Android mirroring per the
  cross-platform rule with Maestro runs.
- Report any gate that cannot run, per the test-stack rules.

## Rollout

One release: host + all four clients + docs + PostHog events land together, per the repo's
versioning and e2e rules. `dev` was already on the next release's version (2.51.0) when this
landed, so it ships with it and needs no bump of its own. Phase 2 items are separate releases.
The user manual's group page covers the team folder when phase 1 ships
(`docs/guides/user-docs.md`).

## Open questions

- **Host-allocated team folder** ("Own folder" for groups, with versioning of the team
  space): design the versioning cadence (per room turn?) before offering; phase 1 deliberately
  avoids inventing folder lifecycles.
- **Team memory**: a group-level memory layer (`groups/<id>/memory/`) opened in every member's
  session — adjacent to codync-mem's shared layer; decide whether workspace notes belong to
  memory or to a plain text field on the group.
- **Per-member scope labels** inside the group (soft ownership hints in the prompt):
  promising for larger teams; hold until real usage shows collision patterns.
