# Bot home and projects

Status: **decided**, 2026-11-24; stage 1 (recap on a folder change) built on `feat/bot-home-projects`, the rest not built. Answers part of stage A in
[personal agent: daily and project work](personal-agent-experience.md): where a bot lives, where its work happens, and
what changes when the user picks a folder.

## Problem

A bot has one folder, `cwd` (`BotConfig::cwd`): its **own folder** (`~/.codync/bots/<id>/workspace`, the default)
or a **project folder** the user picks in its settings. That one field means two things at once: where the bot *is*
(its chat, routines, calls, group turns) and where *code work* happens.

What that costs today:

- **Moving the desk wipes the chat's context.** Any `cwd` change restarts the agent (`agent/bot/queue.rs`, the
  `restart` check) with a fresh session (`restart_agent` → `forget_session`). Unlike an agent or login switch
  (`switched_by_user` in `agent/bot/fallback/handoff.rs`), no recap is carried: the chat still shows the
  conversation, the agent doesn't know it.
- **Daily work lands in the repository.** With a project folder, the chat, every routine run and every group turn
  start in the user's checkout. Files the bot writes for itself go there; there are no versions (only the own folder
  keeps them).
- **The bot can't send work elsewhere.** `add_task` (`chat/work/tools.rs`) has no folder: the bot's threads always
  start in the bot's `cwd`. Only a thread the *user* starts from the Threads page can name a folder
  (`startThread` `setup.cwd`). So a bot that should work on two repositories needs two bots, or a project folder
  switch with the reset above.

## Proposal

Split the two meanings:

| | Today | Proposed |
| --- | --- | --- |
| Bot's `cwd` | Own folder or a project | **Always its own folder** (its home) |
| Project folders | One, as the bot's `cwd` | A list on the bot, `projects` (0..n), the first one its default |
| Bot's own threads (`add_task`) | The bot's `cwd` | A project the bot names (`folder`), else its home |
| Threads the user starts | Bot's `cwd` or a picked folder | The bot's default project, else its home; any folder can still be picked |
| Routine runs, group turns, calls | Bot's `cwd` | Home; a routine may name a project (below) |

The bot becomes the coordinator the product already describes (bot = daily work and dispatch, threads = the work):
it stays home, keeps its context and versions, and sends code work out to threads in the project.

### What stays where

```
~/.codync/
  bots/<bot id>/
    workspace/          the bot's cwd, always (git versions per turn, uploads/ inside)
    bot-memory/         this bot's memory
    artifacts/, files/  what it published / file snapshots
  shared-memory/        "yours", shared by every bot that uses it
  worktrees/<bot id>/<task id>/   worktrees Codync makes in a project (below)

<a project the user owns>/   never written by Codync itself; threads work here or in a worktree of it
```

Nothing of the bot's (identity, memory, uploads) ever goes into a project folder, as today.

### Worktrees in a project

Today's rules ([work: worktrees](../features/work.md#worktrees)), applied to the folder the *thread* works in. The
code already makes the worktree from the thread's folder (`worktree::create(&cfg.cwd, …)` on the thread's config,
`chat/work/worktree.rs`); two rules need changing for threads spread over several folders (below the list):

- **Where**: `~/.codync/worktrees/<bot id>/<task id>`, on a new branch `codync/<title-in-lowercase-dashes>` (`-2`,
  `-3` when taken), from the commit the project folder is on. Uncommitted changes in the user's checkout are not in
  it. When the thread's folder is a subfolder of the repository, the agent starts in the same subfolder of the
  worktree.
- **When**: the project is a git repository, the bot's **Worktree per thread** (`threadWorktrees`) is on, the task is
  the bot's own or the user's, and **another thread of the bot is already working**. The first thread alone works in
  the user's checkout. Routine runs and other bots' requests never get one; a task that follows another (`after`)
  stays in that one's folder.
- **After**: a worktree with nothing uncommitted whose branch is unchanged or merged is removed with its branch;
  anything else is kept and the task's report names the branch and the folder. At host start kept worktrees are
  checked again, and orphans go once merged (a week after their last task, or when the bot is deleted). Codync never
  merges, rebases, resets or force-deletes a branch with work on it, and never touches a worktree it didn't make.
- **Fails**: no commit yet, git missing, a checkout hook fails: the thread works in the project folder itself and
  the failure is logged.

Changes needed (`chat/work/dispatch.rs`):

- **Count side by side per repository.** `side_by_side` is `tasks::working(tasks) > 0`: any working thread of the
  bot, wherever it works. With the bot at home and threads in several projects, a thread alone in repository B would
  get a worktree because one works in A or at home. Count only working threads whose folder is in the same
  repository (same `git rev-parse --show-toplevel`).
- **Fork only into the chat's own folder.** `forks` checks `place == Folder` but not the folder: an `add_task` with a
  project folder would fork the chat's session (started at home) into another folder, which no agent is checked to
  honor (the `ponytail` note there). Add `thread.cwd == cfg.cwd`; a thread in a project starts from its brief, as a
  worktree thread does today.

### Worktree setting

`threadWorktrees` (a bool, on unless set) becomes `worktreeMode`: `off | sideBySide | always`, an enum as the host's
conventions ask, `sideBySide` unless set. In `dispatch.rs` `beside` becomes: `always`, or `sideBySide` and another
thread works in the same repository (above). Routine runs, other bots' requests and a task that follows another keep
their rules under every mode. The wire keeps `threadWorktrees` for older apps (true unless `off`; an older app
turning it on from `off` sets `sideBySide`, turning it off sets `off`, so it never drops `always` by saving
another field). Settings shows a three-way choice under **Projects** when any project is a git repository, with one
line on what each does. Analytics: `worktree_mode` on `bot_updated`'s changed fields, and `work_added`'s `place`.

### Host

- `BotConfig`: `projects: Vec<String>` (absolute, existing folders, validated like `cwd` is in `hub/validate.rs`;
  registry JSON stays untrusted). `cwd` is always `agent::workspace::path(id)`; `prepare` keeps making it.
- `add_task` gains `folder` (optional absolute path). The bot's instructions (`chat/context.rs`, the "Workspace"
  line) list its projects and say: code work in a project goes to a task with that folder; read a project's files
  with explicit paths; never change its own folder.
- `startThread` without `setup.cwd` uses the bot's first project, else its home.
- Routines: optional `folder` on a routine (a project, or home when absent). Today a run uses the bot's `cwd`.
- Everything that creates or edits a bot with a project `cwd` maps it into `projects`: `createBot`/`updateBot`,
  the manage tools (`chat/manage.rs`), import (`import/mod.rs`), templates and the marketplace check
  (`market/verify.rs`).
- What reads `cfg.cwd` today and means "the project" moves to the thread's or the bot's default project:
  the terminal tab (`api/bots.rs` `hub.terms.shell`), a call's "Folder: … (project …)" line
  (`chat/calls/context.rs`), the team directory (`chat/team/directory.rs`), `start_dev_server`'s base
  (`dev_servers/tools.rs`, from a thread it is already the thread's folder), `gitRepo` on the wire
  (`hub/sync.rs`: becomes per project).
- `session.rs` `avoid_clashes` reads MCP server names from the folder the session starts in: at home it no longer
  sees a project's `.mcp.json`; a thread in the project does.
- **The recap on folder change** is built anyway (the migration below moves every project bot once). Not a one-line
  change: `switched_by_user` returns early when the login is unchanged, and `owe_recap` needs a live `session_id`.
  A migration at host start changes the stored `cwd` before the actor runs, so neither fires; the migration has to
  leave a "recap owed" mark in the store that the actor reads at start.

### Migration

At host start, once: a bot whose `cwd` isn't its own folder gets `projects = [cwd]` and `cwd = ` its own folder
(made and versioned as for a new bot), and a "recap owed" mark (above). Groups and threads are not touched; a working
thread keeps its folder until its task ends. Moving the bot home flips `workspace::is_managed`, so the migration also:

- **Stamps `sharedMemory: false`** where it is unset. It is derived when unset
  (`chat/memory/mod.rs`: `shared_memory.unwrap_or_else(|| is_managed(cfg))`), so a project bot would silently start
  reading and writing "yours".
- **Moves its uploads.** `uploads::root` is `<cwd>/uploads` for a managed bot and `bots/<id>/uploads` otherwise:
  files sent before the move would no longer resolve. Move `bots/<id>/uploads` into `workspace/uploads`.
- **Stamps each of its routines with the old folder** (`folder`, below), so a routine keeps running where it did.
- Notes that the harness's own memory changes folder: `own_memory` now writes its switch into the home folder, and
  the Memory screen's Claude Code section (`chat/memory/api.rs` `project_memory`, keyed by `cwd`) shows the home's
  Claude memory, not the project's.

### Wire and compatibility

Additive ([compatibility](../reference/compatibility.md): new fields need no floor change): bots gain `projects`;
`cwd` stays (now always the home). An older app that sets `cwd` to a project gets it moved into `projects` as the
first one by the host, so its "Choose project folder…" keeps doing what the user meant; it shows **Own folder**
afterwards and can't list or remove projects. Whether that is acceptable without raising `minApp` is decision 5.

### Clients (all four, same change)

- Bot settings, **Workspace** section → **Projects**: the list (add with the existing folder picker and its recent
  workspaces, remove, make default), **Worktree per thread** under it when a project is a git repository. **Own
  folder** is no longer a choice; the home path shows read-only with a copy action.
- New thread (desktop Threads page; phones and TUI where they start threads): the folder chip lists the bot's projects
  first, then recent, then browse.
- Task rows / thread headers already show `folder` and `worktree`; no change.
- Desktop, iPhone, Android, terminal UI; user manual (`web/content/docs/bots.mdx` "Workspace", `working-folders`,
  `conversations.mdx` work), and the site's mockups that show bot settings.

### Analytics

`bot_project_added` / `bot_project_removed` (count only), `work_added` gains `folder: home | project | other`,
`bot_projects_migrated` (count) at host start. Never paths.

## What the chat gives up

In a project folder today, the chat's agent loads the project's own `CLAUDE.md` / `AGENTS.md`, `.mcp.json` and
project skills, and Claude's own memory for that folder (`~/.claude/projects/<cwd>`). At home it doesn't; threads in
the project still do. Questions about the code are then answered by reading with explicit paths or by a thread. This
is the point of the split (daily context stays clean), but it is a visible change for users who chat with a "coding
bot"; the manual should say it.

## Decisions (owner, 2026-11-24)

1. **When a thread gets a worktree is the user's choice**, per bot: **Off**, **When threads run side by side** (today's
   rule, the default) or **Always** (every thread of the bot's own or the user's in a project gets one, the first
   included; the user's checkout is never touched). See [worktree setting](#worktree-setting).
2. **Project list per bot.** A computer-wide list is a later step of the personal-agent plan.
3. **`add_task` takes any absolute path**; the bot's projects are what it proposes.
4. **Readable home names** (`bots/<name-slug>` instead of `<uuid>`): a separate change, not part of this one.
5. **No `minApp` raise**: an older app can add a project through its old picker but can't see the list.

## Rollout

Minor version (host + every client). Stages, each shippable, in this order: (1) recap on `cwd` change (with the
"recap owed" mark); (2) routine `folder`; (3) `projects`, `add_task` folder, the two dispatch fixes, `worktreeMode`, prompt,
and the clients' Projects section; (4) migration (needs 1 and 2, so routines and the chat's context survive it).
