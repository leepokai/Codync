# Bots, projects and threads

Status (2026-11-25): every part below is built on `dev`; only [Open](#open) is left for the owner. One plan for where a bot lives, where its work happens and what a thread is given. It replaces
the separate "bot home and projects" and "thread profile" plans. Answers part of stage A in
[personal agent: daily and project work](personal-agent-experience.md). What is built, decided or only proposed:

| Part | State |
| --- | --- |
| Recap when a bot's folder changes | **Built**, in dev (`a5fccdbf7`) |
| [Bot home and projects](#3-bot-home-and-projects) | **Built** (stages 5 and 6), host and all four clients |
| [Worktree mode](#worktree-mode) (Off / Side by side / Always) | **Built** (stage 5), host and all four clients |
| [Worktrees counted per repository](#count-side-by-side-per-repository) | **Built** (stage 5) |
| [Thread profile, no fork](#7-thread-profile-no-fork) | **Built** (stage 2) |
| [A user's thread leaves the bot's queue](#6-a-users-thread-leaves-the-bots-queue) | **Built** (stage 3) |
| [A task names its place](#a-task-names-its-place) (`add_task` `place`) | **Built** (stage 7) |
| [Filling a new worktree](#filling-a-new-worktree) (`.worktreeinclude`, shared folders) | **Built** (stage 7) |
| [Threads without a bot get a folder of their own](#5-threads-without-a-bot-get-a-folder-of-their-own) | **Built** (stage 7) |
| [`ask_task`](#8-the-bot-watching-its-threads): the bot asks a thread and gets an answer | **Built** (stage 7) |

## Words

As in the [Names](../architecture/file-structure.md#names) table; this plan uses no others.

| Said in discussion | The word | In code |
| --- | --- | --- |
| coordinator, main bot | **bot**; where the user talks to it: **chat** | `BotConfig` |
| worker, thread-worker | **thread** | a thread bot: `BotConfig::thread_of`, id `thread-<uuid>` |
| one piece of work | **task** | `Task` |
| what a thread is told | **brief** | `brief` |
| what it hands back | **report** | `report` |
| the queue | **event box** | kv `work.<bot>` |
| what an agent remembers now | **session** | a thread *has* sessions, never *is* one |
| project folder | **working folder**: a **project** (the user's) or an **own folder** (Codync's) | `cwd` |

"Coordinator" is on the Names table's Never list (for chat); "worker" is no product word.

## 1. The model

```
bot ─▶ project ─▶ thread ─▶ sessions
```

- **Bot**: hands work out and keeps its chat free. It lives in its own folder; it can send work to several projects,
  one of them its default.
- **Project**: where code work lives. A thread's work is always in a project (or the thread's own folder); a git
  worktree of a project is Codync's to manage, kept in Codync's data folder (`~/.codync/worktrees/<bot>/<task>`,
  `~/.codync-dev/…` for Codync Dev; `chat/work/worktree.rs` `root()`), never inside the project.
- **Thread**: does one task at a time, may go on to the next (`after`), and is told only what its task needs.
- **Sessions**: a fallback or a restart gives a thread a new session; the thread goes on.

Three ways work starts:

| | The bot hands it out (`add_task`) | The user starts a thread with a bot | The user starts a thread without a bot |
| --- | --- | --- | --- |
| Where it waits | The bot's event box, under `maxThreads` | No wait: starts at once ([§6](#6-a-users-thread-leaves-the-bots-queue)) | No wait: starts at once, no ceiling on the computer |
| What the thread is told | The bot's brief ([§7](#7-thread-profile-no-fork)) | The bot's profile and memory, then the user's own words ([§7](#a-users-thread-is-the-bot)) | The user's own words, as written, nothing of Codync's |
| What it has of the bot | Skills, memory (to look up), tools for its source | Name, description, skills, memory opening, the same tools | Nothing: the agent's own tools |
| Its result | A report the bot turns into a message in the chat | Read in the thread | Read in the thread |
| The bot sees and steers it | Yes | No | There is no bot |

## 2. Before this plan (dev, 2026-11-24)

What the code did, read from it, before any change here. Kept as the reason for each part; what runs now is §1 and
[work](../features/work.md).

### Where a thread works

| Who started it | Its folder |
| --- | --- |
| The bot (`add_task`) | The bot's `cwd`: its own folder (`~/.codync/bots/<id>/workspace`) or its project. `add_task` takes no folder |
| The user, with a bot | `setup.cwd` if picked, else the bot's `cwd` |
| The user, without a bot | The folder picked, else the user's **home folder** (`ThreadStart.tsx` sends `cwd: home`; the box `work::plain` has `cwd` = home) |
| A task that follows another (`after`) | That thread's folder or worktree |
| A routine's run, another bot's request | The bot's `cwd`, never a worktree |

It gets a worktree only when all hold (`dispatch::plan`, `beside`): the folder is a git repository, the bot's
`threadWorktrees` is on, the folder isn't the bot's own, and **another thread of the bot is already working**
(`side_by_side` = any working thread of the bot, in any folder). So the first thread works in the user's checkout.

### What a thread is given

- **The bot's whole system prompt** (`agent/bot/session.rs` renders `context::render(&hub.store, &self.cfg.as_owner())`;
  only the Workspace line is the thread's): name and description, the Environment paragraph, Messages, Other
  conversations, Other computers, Workspace, `dev_servers::BRIEF`, `routines::BRIEF`, **`work::BRIEF`** (call
  `add_task`), skills, `KEEP`, the bot's memory opening.
- **A contradiction**: `work::BRIEF` tells it to hand work on with `add_task`, which is refused in a thread; the task
  prompt (`work::prompts::task_prompt`) overrides it each time.
- **The bot's whole tool set**: `connectors`, `chat`, `team`, `memory`, `routines`, `workflows`, Composio
  (`Session::mcp_servers`); only `computer` and `manage` are off (`dispatch::thread_bot`). What a thread may not
  do is refused **when called** (`work::refuse_in_thread`, `refuse_chat_only`, `acting_as`), so it reads the box,
  profile, fallback, `team` and `routines` tool descriptions for nothing.
- **A fork of the chat's session** for the bot's own task in its folder on its own agent and login
  (`task.fork_from`, `dispatch::fork_source`, `sources::from_chat`): the thread carries the whole chat.

### How a user's thread is scheduled

`startThread` with a bot (`sources::add_thread`, `Source::User`) puts a task in the bot's box: it waits for a place
under `maxThreads` beside the bot's own tasks and for the bot's own turn to end (`dispatch::standings`,
`capacity.chat_free`), and the bot reads it in the box summary of every turn (`tasks::lines` → `shown`, no filter
by source) and in `work_status`. Without a bot it goes to the hidden box `Agents` (`BotConfig::plain`), never
waits on a bot, and shares that box's 6 places.

### Where it can go wrong

From the code above, most harm first:

1. **The first thread works in the user's checkout.** No worktree until a second thread runs; the user editing the
   same files at the same time collides with it. ([Worktree mode](#worktree-mode) **Always** fixes it.)
2. **A new worktree is empty of what isn't committed**: no `node_modules`, no `.env`, none of the user's
   uncommitted changes. A thread reinstalls or fails on missing config. ([Filling a new worktree](#filling-a-new-worktree).)
3. **Handing work out is a prompt rule only** (`work::BRIEF`, `EACH_TURN`): a bot that does long work in its chat
   leaves the user without it until the turn ends.
4. **A user's thread queues behind the bot's work** and is shown to the bot. ([§6](#6-a-users-thread-leaves-the-bots-queue).)
5. **A bot reaches one folder**: `add_task` has no folder. ([§3](#3-bot-home-and-projects).)
6. **A thread carries the chat**: by fork, every word of it; without one, the whole chat prompt and tool list.
   ([§7](#7-thread-profile-no-fork).)
7. **Kept worktrees pile up**: they stay until merged and are looked at again only at host start.
8. **Side by side is counted across repositories**: a thread in B gets a worktree because one works in A (the
   `ponytail` note in `dispatch.rs`).
9. **Threads without a bot share the home folder**: up to 6 at once in `~`, no versions. ([§5](#5-threads-without-a-bot-get-a-folder-of-their-own).)

## 3. Bot home and projects

### Problem

A bot has one folder, `cwd`: its **own folder** (`~/.codync/bots/<id>/workspace`, the default) or a **project** the
user picks in its settings. That one field means where the bot *is* (its chat, routines, calls, group turns) and
where *code work* happens.

- **Moving the desk reset the chat's context.** Any `cwd` change restarts the agent (`agent/bot/queue.rs`, the
  `restart` check) with a fresh session. Fixed: the recap now carries over (`a5fccdbf7`).
- **Daily work lands in the repository.** With a project as `cwd`, the chat, every routine run and every group turn
  start in the user's checkout; files the bot writes for itself go there, with no versions.
- **The bot can't send work elsewhere.** `add_task` has no folder, so a bot that should work on two repositories
  needs two bots.

What a project as `cwd` gives today, and keeps giving to threads after the change: the agent loads the project's
`CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills and the agent's own memory for that folder.

### Proposal

| | Today | Proposed |
| --- | --- | --- |
| Bot's `cwd` | Own folder or a project | **Always its own folder** (its home) |
| Projects | One, as the bot's `cwd` | A list on the bot, `projects` (0..n), the first its default |
| Bot's own threads (`add_task`) | The bot's `cwd` | A folder the bot names (`folder`), else its default project, else its home |
| Threads the user starts with the bot | Bot's `cwd` or a picked folder | The bot's default project, else its home; any folder can still be picked |
| Routine runs, group turns, calls | Bot's `cwd` | Home; a routine may name a folder |

```
~/.codync/
  bots/<bot id>/
    workspace/          the bot's cwd, always (git versions per turn, uploads/ inside)
    bot-memory/         this bot's memory
    artifacts/, files/  what it published / file snapshots
  shared-memory/        "yours", shared by every bot that uses it
  worktrees/<bot id>/<task id>/   worktrees Codync makes in a project
  threads/<task id>/    a thread without a bot (§5, proposed)

<a project the user owns>/   never written by Codync itself; threads work here or in a worktree of it
```

Nothing of the bot's (identity, memory, uploads) goes into a project.

### Host

- `BotConfig`: `projects: Vec<String>` (absolute, existing folders, validated like `cwd` in `hub/validate.rs`;
  registry JSON stays untrusted). `cwd` is always `agent::workspace::path(id)`.
- `add_task` gains `folder` (any absolute path; decision 3) and `place` ([below](#a-task-names-its-place)). The bot's
  instructions (`chat/context.rs`, the Workspace line) list its projects and say: code work in a project goes to a
  task with that folder; read a project's files with explicit paths; never change its own folder.
- `startThread` without `setup.cwd` uses the bot's first project, else its home.
- Routines: optional `folder` (a project, or home when absent).
- Everything that creates or edits a bot with a project `cwd` maps it into `projects`: `createBot`/`updateBot`,
  the manage tools (`chat/manage.rs`), import (`import/mod.rs`), templates and the marketplace check
  (`market/verify.rs`).
- What reads `cfg.cwd` and means "the project" moves to the thread's or the default project: the terminal tab
  (`api/bots.rs` `hub.terms.shell`), a call's "Folder" line (`chat/calls/context.rs`), the team directory
  (`chat/team/directory.rs`), `start_dev_server`'s base (`dev_servers/tools.rs`), `gitRepo` on the wire
  (`hub/sync.rs`: per project).
- `session.rs` `avoid_clashes` reads MCP server names from the folder a session starts in: at home it no longer sees
  a project's `.mcp.json`; a thread in the project does.

### Migration

At host start, once: a bot whose `cwd` isn't its own folder gets `projects = [cwd]`, `cwd` = its own folder (made and
versioned as for a new bot), and a "recap owed" mark the actor reads at start (the recap needs a live session, which a
migration before the actor runs doesn't have). Groups and threads are not touched; a working thread keeps its folder
until its task ends. Moving the bot home flips `workspace::is_managed`, so the migration also:

- **Stamps `sharedMemory: false`** where unset (it is derived from `is_managed` when unset, `chat/memory/mod.rs`),
  so a project bot doesn't silently start reading and writing "yours".
- **Moves its uploads**: `bots/<id>/uploads` into `workspace/uploads` (`uploads::root` changes with `is_managed`).
- **Stamps each routine with the old folder**, so it keeps running where it did; not on a bot whose routines run in
  its chat (computer use, Manage the team), whose runs keep those tools and run at home.
- Notes that the harness's own memory changes folder: `own_memory` writes its switch into the home folder, and the
  Memory screen's Claude Code section (`chat/memory/api.rs` `project_memory`, keyed by `cwd`) shows the home's.

### What the chat gives up

At home the chat's agent no longer loads the project's `CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills or
the agent's memory for that folder; threads in the project still do. Questions about the code are answered by
reading with explicit paths or by a thread. That is the point (daily context stays clean), and a visible change for
users who chat with a "coding bot": the manual says it.

### Wire and compatibility

Additive ([compatibility](../reference/compatibility.md)): bots gain `projects`; `cwd` stays (always the home). An
older app that sets `cwd` to a project gets it moved into `projects` as the first one, so its "Choose project
folder…" keeps doing what the user meant; it shows **Own folder** afterwards and can't list projects (decision 5).

### Clients (all four, same change)

- Bot settings, **Workspace** → **Projects**: the list (add with the folder picker and its recent workspaces, remove,
  make default) and the [worktree mode](#worktree-mode) under it when a project is a git repository. **Own folder** is
  no longer a choice; the home path shows read-only with a copy action.
- New thread (desktop Threads page; phones and TUI once they start threads): the folder chip lists the bot's projects
  first, then recent, then browse.
- Desktop, iPhone, Android, terminal UI; user manual (`web/content/docs/bots.mdx` "Workspace", `working-folders`,
  `conversations.mdx`), and the site's mockups of bot settings.

### Analytics

`bot_project_added` / `bot_project_removed` (count only), `work_added` gains `folder: home | project | other`,
`bot_projects_migrated` (count). Never paths.

## 4. Worktrees

### The rules, applied to the thread's folder

- **Where**: `~/.codync/worktrees/<bot id>/<task id>`, branch `codync/<title-in-lowercase-dashes>` (`-2`, `-3` when
  taken), from the commit the project is on. When the thread's folder is a subfolder of the repository, the agent
  starts in the same subfolder of the worktree.
- **When**: by the bot's [worktree mode](#worktree-mode), or the task's own [place](#a-task-names-its-place). Routine
  runs and other bots' requests never get one; a task that follows another stays in that one's folder; the bot's
  home and a thread's own folder never get one.
- **After**: a worktree with nothing uncommitted whose branch is unchanged or merged is removed with its branch;
  anything else is kept and the report names the branch and folder. Orphans go once merged (a week after their last
  task, or when the bot is deleted). Codync never merges, rebases, resets or force-deletes a branch with work on it,
  and never touches a worktree it didn't make.
- **Fails**: no commit yet, git missing, a checkout hook fails: the thread works in the folder itself, logged.

### Worktree mode

Decided (1). `threadWorktrees` (a bool) becomes `worktreeMode`: `off | sideBySide | always`, `sideBySide` unless set.
`beside` in `dispatch.rs` becomes: `always`, or `sideBySide` and another thread works in the same repository.
**Always** gives every thread of the bot's own work or the user's a worktree, the first included, so the user's
checkout is never touched. The wire keeps `threadWorktrees` for older apps (true unless `off`; turning it on from
`off` sets `sideBySide`, off sets `off`, so saving another field never drops `always`). Settings shows the three
choices with one line each. Analytics: `worktree_mode` on `bot_updated`, `place` on `work_added`.

### Count side by side per repository

Decided. `side_by_side` is `tasks::working(tasks) > 0` today. With threads in several projects, count only working
threads whose folder is in the same repository (same `git rev-parse --show-toplevel`). Removes the `ponytail` note.

### A task names its place

Built (stage 7). `add_task` gains `place: here | worktree` (absent: the bot's mode decides). The bot knows whether a task
edits code the user may be touching, or should build on the work of the thread before it; the mode is the default,
the task the exception. `startThread`'s setup takes the same choice from the new-thread page.

### Filling a new worktree

Built (stage 7). A new worktree is a clean checkout: no ignored folders (`node_modules`, `target`, `.venv`), no local
config (`.env`), none of the user's uncommitted changes. Two lists the project keeps, read from the project's main
checkout:

- **`.worktreeinclude`** at the repository root: ignored files to **copy** into each new worktree (`.env`,
  `.env.local`, editor settings). Literal paths only; a path that is tracked, missing or not ignored is skipped.
- **Shared folders**: ignored folders to **link** into each new worktree (`node_modules`, `.cache`), set per project
  in Codync (a project row's settings) so nothing Codync-specific has to be committed.

Copied or linked before the agent starts, so the first turn finds them. The user's uncommitted changes are never
carried: the report says the worktree started from the last commit. Each copy or link failure is logged and counted
(`worktree_fill_failed`, `stage`), never fatal.

### Cleanup

Kept worktrees were looked at again only at host start. With threads in several
projects that can pile up, so the hourly look (`sweep.rs`) also checks them. Built with projects (stage 5).

### How another tool does it

Orca (an open-source agent IDE, read from its docs and `worktree-logic.ts`, 2026-11-24): every task gets a worktree,
never the main checkout; the one handing out work names its place each time (`--worktree current | new-child |
new-top-level`, `--repo` for another project, `--on` for another computer); worktrees live outside the
repository under one folder (`<workspaceDir>/<repo>/<name>`, per repository overridable); and `.worktreeinclude`,
shared folders and a setup script fill a new one before the agent starts. [A task names its place](#a-task-names-its-place)
and [filling a new worktree](#filling-a-new-worktree) take the two ideas that fix the first two problems in [§2](#where-it-can-go-wrong).

## 5. Threads without a bot get a folder of their own

Built (stage 7). Before, such a thread worked in the user's home folder unless one was picked (§2): no separation
between up to 6 at once, no versions, and a home folder kept in git (dotfiles) can be given a worktree of itself.

Each thread without a bot gets `<data dir>/threads/<task id>/`, made when its thread starts and versioned like a bot's
own folder (`agent::versions`: `git init`, the default `.gitignore`, a save before and after each turn). It is the
default choice; **Browse other folders** still picks a project.

- Host: `sources::add_thread` fills `setup.cwd` with it when a thread without a bot has none;
  `workspace::own_folder` counts it as the thread's own, so it never gets a worktree. It goes with the thread bot
  (when the task is deleted with its transcript: [older threads](../features/work.md#older-threads)).
- Desktop: the folder choice's first item reads **Own folder** instead of the home folder's name; the app stops
  sending `cwd: home` only when `hello` says the host makes the folder (a new flag), so an older host keeps today's.
- Phones and TUI don't start threads today: nothing to change.
- Analytics: `work_added`'s `folder` is `own | other` for threads without a bot.
- Open: whether its files outlive the thread.

## 6. A user's thread leaves the bot's queue

Decided (owner, 2026-11-24; replaces the same day's "keep the shared scheduling"). A thread the user starts is the
user's, not the bot's dispatch. It keeps its **task record** (stop, steer, try again, `send` to a thread,
`recall_task` for the user's apps, the watchdog, the hourly look and the worktree rules are built on it) and leaves
the **queue** and the bot's **view**.

| | Today | Decided |
| --- | --- | --- |
| Where the task lives | The bot's box | Same box, same `Task`, `source: user` |
| Counts toward `maxThreads` | Yes | **No** |
| Waits for the bot's own turn | Yes (`chat_free`) | **No**: nothing of that turn is in it, and stopping that turn can't drop it |
| Near the usage limit | One thread at a time | Unchanged |
| Counts for side by side (worktrees) | Yes | Yes: two threads in one repository still need two folders |
| In the bot's box summary, `work_status`, `recall_task` | Yes | **No** |
| `steer_task` / `tell_task` / `ask_task` from the bot | Allowed | Refused: not the bot's |
| On the Threads page, steering, messages | As a thread | Unchanged |

- `dispatch::standings`: a waiting `Source::User` task starts whenever it isn't `held` and the usage guard allows,
  without taking from `free`. Tests: `a_users_thread_starts_past_the_ceiling`.
- `tasks::shown` gets a view for the bot that leaves `Source::User` out (`lines`, `work_status`, `recall`); the
  clients' wire keeps every task.
- `steer.rs` / `thread.rs` `tell`: refuse `Source::User` when the bot's agent calls; the user's own `workFirst`,
  `stopWork`, `retryWork`, `send` stay.
- No ceiling is a choice: the user started each by hand. If it ever floods a computer, add a per-computer ceiling for
  user threads, not a share of the bot's.
- Analytics: `waited_ms` on a thread's start.

## 7. Thread profile, no fork

Decided (owner, 2026-11-24): a thread is told only what its task needs, and **no thread forks the chat's session**:
the chat talks about many things that have nothing to do with the task, and the brief is the one place what the task
needs is written.

### What every thread gets

`chat/context.rs` `render_thread(store, cfg, task)` beside `render` (which stays the chat's):

| Part | What it says |
| --- | --- |
| The task | The brief (the bot's, a routine's prompt, another bot's request) or the user's own words |
| Where | Its folder or worktree and branch; not to change the bot's settings to work elsewhere |
| How it ends | Its last message is its report (to the bot's chat, or kept in the thread); `request_decision` when it needs the user; `send_file` and artifacts to share |
| Skills | The bot's skills brief (`market::skills_brief`): how the work gets done |
| Memory | Not in the prompt: the `memory` server to look things up, and the brief carries what the bot knows matters. A user's thread is the exception ([below](#a-users-thread-is-the-bot)) |
| Environment | One line: it runs inside Codync, so it doesn't point the user at the agent's own CLI |
| Dev servers | `dev_servers::BRIEF`: a server it starts is the bot's, listed and stopped there |

No thread gets: Messages, Other conversations, Other computers, `routines::BRIEF`, `work::BRIEF`, the chat so far.
The bot's name, description and memory opening go only to a user's thread ([below](#a-users-thread-is-the-bot)).

### A user's thread is the bot

Decided (owner, 2026-11-24): a thread the user starts with a bot is the user talking to **that bot**, in a new
conversation in the thread's folder. The bot handed nothing out and wrote no brief, so the thread can't be told
"only the task": it is told who it is, as the bot is.

| | Bot's task (brief) | User's thread |
| --- | --- | --- |
| Who it is | A thread doing one task; no name | **The bot**: its name and description (the bot's own instructions) |
| Memory | Looked up with the `memory` server | **The bot's memory opening** (its own layer and "yours", as the chat gets it), and the `memory` server |
| Skills, Environment, Workspace, dev servers | ✔ | ✔ |
| The project's `CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills | Loaded by the agent from its folder | Same |
| The chat so far | ✗ | ✗: a new conversation |
| Messages, Other conversations, Other computers, `routines::BRIEF` | ✗ | ✗: it has no chat, room or request to answer |
| `work::BRIEF`, box tools (`add_task`, …) | ✗ | ✗: a thread runs no threads |
| How it ends | Report to the bot's chat | The user reads its answers here and may write to it |

So `render_thread` for `Source::User` is the chat's profile part (`render`'s name, description, Environment,
Workspace, dev servers, skills, `KEEP`, memory opening) without its chat-only paragraphs, plus the "How it ends" line
for a thread the user talks to. The setup the user picked for it (model, approvals, login, agent) applies to this
thread only; the bot's own settings don't change. The bot itself still doesn't see the thread (§6).

### By source

| | Bot's task | User's thread | Routine run | Bot request |
| --- | --- | --- | --- | --- |
| `chat`: `request_decision`, `send_file`, artifacts | ✔ | ✔ | as today | as today |
| `chat`: box, profile, fallback tools | ✗ | ✗ | ✗ | ✗ |
| `memory`, connectors, Composio, `workflows` | ✔ | ✔ | ✔ | ✔ |
| `routines` + its paragraph | ✗ | ✗ | ✔ (progress note, follow-ups) | ✔ (an asked bot runs its own routine) |
| `team` (`list_bots`, `message_bot`) | ✗ | ✗ | ✔ | ✔ (`message_bot` refused with why, as in the chat) |
| `computer`, `manage` | ✗ | ✗ | ✗ | ✗ |

The routine and request columns are today's rules (`work.md`, "Other sources") written as a profile.

### The tools, by name

Besides the agent's own tools (files, shell, web) and what the agent loads from its folder itself (the project's
`CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills):

| Server | A thread lists | A thread doesn't list |
| --- | --- | --- |
| `chat` | `request_decision`, `send_file`, `publish_artifact`, `list_artifacts`, `start_dev_server`, `list_dev_servers`, `dev_server_logs`, `stop_dev_server` | `send_message` (its report is its message), the reaction tool (no chat message to react to), the save-a-skill tool (open: a thread that worked out a way of working says so in its report), `add_task`, `steer_task`, `tell_task`, `work_status`, `recall_task`, the profile and fallback tools |
| `memory` | `search_memory`, `read_memory`, `search_history`, `edit_memory` (writes the bot's memory as the thread's own turn, as today) | |
| connectors, Composio, `workflows` | as the bot has them | |
| `routines`, `team` | only on a routine's run or a request (table above) | otherwise |
| `computer`, `manage` | never | |

### The prompt, a draft

The system prompt of a thread on the bot's own task (`render_thread`); the other sources change the "How it ends"
paragraph and add their own (routine, request):

```text
You are doing one task for the user, through Codync. The user talks to Codync from its phone, desktop or terminal
app, not in <agent>'s own CLI, so don't point them at its commands or shortcuts.

Task: everything you know about it is in the message below. You don't see the conversation it came from; if
something you need isn't there, look it up (memory, the files) before you ask.

Where: you work in <folder> [a git worktree of <project> on branch codync/<slug>, from its last commit: the user's
uncommitted changes are not in it]. Keep to it; don't change Codync settings to work elsewhere.

How it ends: your last message is your report. It goes to the bot that gave you this task, which tells the user, so
say what you did, what you found, what is left, and where (files, branch). When you need the user to choose, call
request_decision; to hand over a file or a page, send_file or publish_artifact.

Memory: what the user and the bot have kept is searchable with search_memory and read_memory; save what you learn
that should outlast this task with edit_memory.

Dev servers: <dev_servers::BRIEF>

Skills you can use when they fit the request:
- <name>: <description> (read <path>/SKILL.md before using it)

<KEEP>
```

The first message is the brief as today (`work::prompts::task_prompt`, without what it says now to override the
chat's prompt). A user's thread gets the bot's profile instead ([above](#a-users-thread-is-the-bot)).

### How a thread is used

1. **Handed out.** In its chat the bot calls `add_task {title, brief, folder?, place?, after?}`; the task waits in
   the event box. (A user's thread: `startThread`, no wait, §6.)
2. **Started.** The dispatcher has a seat: it makes the thread bot (the bot's config with the task's setup), picks its
   folder or worktree (§4) and fills a new worktree, then opens a new session with `render_thread` and the brief as
   the first message.
3. **Working.** The thread uses the agent's tools in its folder. A choice or approval shows as a card in the bot's
   chat (a user's thread: in the thread), and the thread waits on it, its seat lent to the next task.
4. **Watched.** The bot reads it with `work_status` and `recall_task`, corrects it with `tell_task`, asks with
   `ask_task`, restarts it with `steer_task replace` or ends it with `steer_task stop`. The user can
   open the thread and write to it.
5. **Reported.** Its turn ends: the last message is the report; the bot gets a turn with it and tells the user (a
   user's thread: it stays in the thread). Its worktree is removed or kept (§4).
6. **Next or gone.** A task that follows it (`after`) goes on in the same thread and session; otherwise the thread
   ends (its agent stops, its transcript stays), and it is deleted only as the user's Delete old threads says ([older threads](../features/work.md#older-threads)).

### No fork

- `sources::from_chat` goes; `dispatch::fork_source`, `threads_fork`, `task.fork_from`, `task.fork_failed` and
  the actor's fork path in `agent/bot/session.rs` go with their tests and the `work.md` text about forks. A task
  that follows another (`after`) still goes on in that thread's session.
- `capacity.chat_free` stays for the bot's own tasks: stopping the turn that added a task drops the task, and a
  routine's run or a room's turn shares the actor, so a task still starts once that turn ends. A user's thread
  doesn't wait for it ([§6](#6-a-users-thread-leaves-the-bots-queue)).
- The brief carries the weight: `add_task`'s `brief` description and `work::BRIEF` say the thread knows nothing of
  the chat, so the brief has the goal, what is known, the user's words that matter and what done looks like. The
  user's words of the turn are already kept with the task (`words`, `notes`) and still go to the thread.

### Host

- `render_thread`, with a test per source for what is in and out (never `work::BRIEF`, the bot's name or its memory
  opening).
- `agent/bot/session.rs`: a thread's new session renders `render_thread` from its task; `context::adopt` and the
  snapshot bookkeeping take that text as they take the chat's.
- `Session::mcp_servers`: a thread lists only its source's servers; the `chat` server started with `--thread`
  lists only what a thread may call, `refuse_chat_only` stays as the guard behind it.

### How to check

- Tokens: a new thread's first-turn input tokens before and after, one Claude and one Codex thread.
- Behavior: `thread_tool_refused` (new event, by tool) falls to near zero; measured before the change.
- Quality: the same five briefs before and after; a thread that asks for chat context it no longer has means
  `work::BRIEF` must ask for fuller briefs.

## 8. The bot watching its threads

What the bot has today (`chat` server; for its own tasks, routine runs and requests, not a user's threads after §6):

| To | Tool |
| --- | --- |
| See what each thread does now | `work_status` (status, the thread's activity line, why a task waits) |
| Read a thread's steps | `recall_task {task}` (30 steps a page, `before` for the page before) |
| Correct its direction | `tell_task {task, text}` (after its running turn; `now` into it) |
| Start it again from a new brief | `steer_task replace` |
| End it | `steer_task stop` |
| Reorder | `steer_task first` / `now` |

Missing: a question with an answer. `tell_task`'s words are read, but the thread's answer comes only in its report.

Built (stage 7): `ask_task {task, question}` → `{answer}`. The question goes into the thread's running turn as
`tell_task now` does, marked as the bot asking; the thread answers with one short message that goes back to the bot
as the tool's result (not as the report) and goes on with its task. The bot's turn waits for the answer up to a time
limit (60 s), then gets `no answer yet` and the answer arrives later as a turn of its own, as a `message_bot` reply
does. Refused for an ended task, a user's thread, a routine's run and a request. Analytics: `task_asked` with
`outcome: answered | late | refused`.

## Decisions (owner, 2026-11-24)

1. **Worktree mode per bot**: Off, When threads run side by side (default), Always.
2. **Project list per bot.** A computer-wide list is a later step of the personal-agent plan.
3. **`add_task` takes any absolute path**; the bot's projects are what it proposes.
4. **Readable home names** (`bots/<name-slug>`): a separate change.
5. **No `minApp` raise**: an older app can add a project through its old picker but can't see the list.
6. **A user's thread leaves the bot's queue and the bot's view** (§6).
7. **Threads are told only their task**: the thread profile, skills kept, memory looked up (§7). A thread the user
   starts with a bot is the bot: its name, description, skills and memory opening, without the chat's paragraphs.
8. **No session fork** for any thread (§7).
9. **The words**: bot, chat, task, brief, thread, report, event box; not coordinator or worker.

## Open

- Can a user's thread hand its result to the bot ("tell my bot about this")? A button that posts the thread's result
  into the bot's chat would, without the bot seeing the box.
- Does a user's thread want the bot's connectors, or only its skills and memory?
- Do the files of a thread without a bot outlive it?

## Rollout

Each stage ships on its own, in this order; host-only stages need no client release beyond the version bump.

1. **Recap on a folder change.** Built (`a5fccdbf7`).
2. **Thread profile and no fork** (§7). Host only. Built: a routine's run and another bot's request are told who the
   bot is, as a user's thread is (they act as the bot, not on a brief the bot wrote), with only their own paragraph.
3. **A user's thread leaves the queue** (§6). Host only. Built; `waitedMs` went on `work_finished` (there is no start
   event).
4. **Routine `folder`.** Host + clients (the routine editor). Built: any existing folder for now (projects come in 5);
   a run with one always works in a thread, and a folder that went fails the run.
5. **Projects**: `projects`, `add_task` `folder`, `worktreeMode`, side by side per repository, the Workspace line,
   the clients' Projects section, cleanup in the hourly look (§3, §4). Minor version, every client. Host built: as
   [work: where a thread works](../features/work.md#where-a-thread-works) says. Beyond the plan: the box of threads
   without a bot keeps the home folder as its `cwd` (it gets no projects; stage 7 then gave each of its threads a folder of its own, §5); a thread in a worktree
   doesn't count toward side by side (its checkout is its own); the bot hears a change to its projects as a profile
   update; `projectGitRepos` (bools in `projects`' order) tells the clients which projects are repositories, and
   `gitRepo` means the default project for older apps; schema 14 stores `worktreeMode` in place of the bool.
   `market/verify.rs` keeps checking a connector in the bot's own folder, where the chat's agent starts it now.
   Clients built in the same change (the Projects section and Worktree mode on all four).
6. **Migration** (§3). Needs 4 and 5. Built with 5 (`hub/projects/migrate.rs`). A routine stamped with the old folder
   always runs in a thread, so on a bot that uses the computer or manages the team (whose runs were turns in its chat)
   it loses those tools: that is the price of keeping the folder, kept as the plan says.
7. **A task's place, filling a new worktree, threads without a bot's own folder, `ask_task`** (§4, §5, §8). Host
   and clients built, as [work](../features/work.md) says: the desktop's new-thread place menu (Here / New worktree)
   and Own folder, and the Shared folders editor under each git project on the desktop, iPhone, Android and TUI. Beyond the plan: the place is kept in the task's `setup`, so a task
   tried again or replaced keeps it, and `add_task` answers `placeNote` when the place can't apply; the folder of a
   thread without a bot is made as its task is added and goes when the task is deleted (dropped, or by Delete old threads:
   [older threads](../features/work.md#older-threads)), which is when its thread bot and transcript go; a shared folder is linked only into worktrees of its
   project's repository, at the project's place inside it, and the links Codync made don't keep an untouched
   worktree (each link is listed in the repository's `info/exclude`, so a thread can't commit it); `ask_task` takes one question at a time a task, and a task that ends before it answers counts as
   `late`; a late answer is its own host turn (`Cmd::TaskAnswer`). `hello.threadPlace`, `hello.worktreeFill`,
   `hello.threadFolders` tell the clients.

Docs in the same stages: `docs/features/work.md`, `docs/architecture/conversation-runtime.md` (Work), the user
manual's bots and conversations pages, the site's mockups of bot settings.
