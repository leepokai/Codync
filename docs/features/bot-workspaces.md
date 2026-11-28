# Bot workspaces

New agent bots get their own persistent folder automatically (**Own folder** in the apps). Creating a bot does not require choosing a project. An omitted or empty `cwd` in `createBot` allocates `<CODYNC_HOME>/bots/<bot-id>/workspace` (normally `~/.codync/bots/<bot-id>/workspace`). Groups do not allocate folders. An omitted or empty `backend` gets Claude when it runs on the computer, else the first runnable agent Codync knows (the terminal UI relies on this; the apps still pick it themselves for older hosts), and omitted `connectors`/`skills` turn on every installed one.

The host persists the resolved absolute directory in `cwd` and exposes `managedWorkspace` in bot responses. A bot always lives in its own folder: the user's folders it sends work to are its **projects** (`projects`, the first its default), and a `cwd` set to another folder (an older app's **Choose project folder…**, a managing bot's `folder`) becomes its first project instead ([where a thread works](work.md#where-a-thread-works)). At host start, once, a bot that worked in a project moved home the same way, the project kept.

## Task execution

The bot's own folder is the chat's agent process and ACP session's starting directory. The bot's instructions list its projects: code work in one goes to a task with that folder (a thread working there loads the project's `CLAUDE.md` / `AGENTS.md`, `.mcp.json` and skills; the chat no longer does), and it reads a project's files with explicit paths. This does not modify bot configuration or reset the conversation. Access remains subject to the harness's permissions; a bot's own folder is not a sandbox and does not grant additional access.

Routines and group contributions start in the bot's own folder; a routine may name a folder, where its runs work in a thread. Task-specific paths should be explicit in routine instructions. The host does not infer a task directory from the last chat message. Harness-specific restrictions can still require user approval for other paths.

Changing the bot's own folder (its `cwd`, before projects; now only the move home at host start) starts the bot's agent again there, in a new session (`restart_agent`). Its next message carries a recap of the chat, as after a switch to another agent (`moved_folder` in `agent/bot/fallback/handoff.rs`): the same handoff, kept across a host restart, opening with "the user moved you to the folder you're in now" instead of naming another agent, and with no line in the chat. This is separate from executing a task in another directory.

## Lifecycle

- Renaming a bot or changing its model does not move or erase workspace files.
- Restarting the host or starting a new conversation context preserves the directory.
- Deleting a bot leaves its workspace files on disk. There is no automatic cleanup.
- A bot that worked in a project was moved to its own folder once (a backup of its config and routines first, in `backups/`).

Settings on every client offered **Own folder** or **Choose project folder…** (an older app still does: a project it picks is added as the first); desktop quick creation and the terminal's new-bot flow default to Own folder. Directory selection is optional. Separate workspaces do not prevent conflicts when multiple bots edit the same external project.

## Versions

A bot's own folder is a git repository of its own, so what each turn did can be seen and undone (`agent/versions.rs`). A project folder is never touched: its history is the user's. Neither is a repository already in the bot's own folder (one the bot or the user made before versions): Codync keeps versions only in one it made, marked by a `codync-versions` file in its `.git`.

- **Made** when the bot's actor starts (a new bot, a host start: bots from before versions, git installed since) and when the bot switches back to its own folder: `git init`, the mark, a `.gitignore` for what tools fill again by themselves (`node_modules/`, `.venv/`, `__pycache__/`, `.cache/`, `target/`, `dist/`, `build/`, `.next/`, `.gradle/`, `.turbo/`, `.DS_Store`…) when the folder has none, and a first commit (or the first that saves, when that one fails). The bot is emitted again so `gitRepo` shows. Its own repository even when an outer folder is one (a home folder kept in git): `add` stays inside.
- **Saved** before each turn that works there (`Before: <first line of what it was asked>`, past Codync's own notes such as the message id; waited for up to 10 seconds, then left to finish by itself) and after it (`<first line>`), when something changed: `git add -A --ignore-errors` and a commit. What the bot staged itself is kept (a `git rm --cached` stays untracked), except what is left out below: that is unstaged first (`git reset -- <path>`). Git runs with no time limit and is never killed, not even when the host stops: a killed `git add` leaves its lock, and no version would be saved there again. Each bot or task saves in the order it asked (`Saves`: the next save waits for the last), so a turn's changes never land under the next turn's name. A task working in its bot's folder saves too; one in a worktree doesn't.
- **As Codync**: every command carries `user.name=Codync`, `commit.gpgsign=false`, no hooks (`core.hooksPath` to a folder that isn't there, `--no-verify`), no fsmonitor, whatever the user set globally. One save at a time in a folder; folders don't wait for each other.
- **Left out**: a new or changed file over 50 MB, staged by the bot or not (`:(exclude,literal)` pathspecs), so a download doesn't swell every version, and `uploads/`, the files sent to the bot (`chat::uploads`), some of which Codync deletes on purpose. A file git can't read (a name Windows reserves, such as `nul`; a file locked or not readable) stays out until it can be read, and the rest is saved.
- **Skipped**, not forced, while git is at work there: `index.lock`, a merge, cherry-pick or rebase in progress (the bot's own git use). Each skip is logged and counted (`stage: busy`).
- **Without git** nothing changes: no repository, no versions; the git line in Settings says git isn't installed.
- The bot is told in its instructions (`chat::context`) that its folder keeps versions in git, so it can show or bring back an earlier file when the user asks. There is no restore UI.
- Threads in the bot's own folder never get worktrees, whatever its `worktreeMode`: the bot's work stays in one folder, and overwritten files can be brought back from a version.
- Events: `own_folder_versioned` when a folder gets its first version, `own_folder_save_failed` with its `stage` (`init`/`busy`/`look`/`add`/`unreadable`/`commit`) and `reason` (`permission`/`long_path`/`unreadable_name`/`locked`/`nested_repository`/`other`). A save that leaves out a file git can't read counts as `unreadable`, so one such file never stops every version after it; when nothing could be staged it counts as `add`.

## Workspace picker

The desktop, iPhone and Android picker starts with recent selections for the bot's computer. The host keeps the last 12 unique selections in SQLite (`recent_workspaces`); `selectWorkspace` validates the absolute folder path and its readability before recording it. Selection alone does not update a bot: existing settings save behavior applies. Browsing never adds history.

`listDirs` includes optional `recent` and `ancestors` arrays. Clients use absence of `recent` to detect older hosts and validate selection through `listDirs`. The browser supports home expansion (`~`, `~/`, `~\\`), parent navigation, breadcrumbs, filtering within the current folder, empty states and inline errors. An empty Windows drive-list path cannot be selected. Paths belong to the current bot's host; the computer label is informational, not a host-switch action.

The terminal picker uses Ctrl+R for recent workspaces, Ctrl+B to browse and Ctrl+L to enter a path. Right enters a highlighted folder, Left goes to its parent, Enter confirms the highlighted folder, and `.` uses the current folder. Selection errors keep the picker open.

On the iPhone the picker is native: a sheet (medium and large) with its own navigation stack, titled Choose workspace over "Your bot will work in this folder.", Cancel and Use this folder in the navigation bar, the computer and the recent workspaces as `List` sections searched with the system search field, and Browse other folders pushing one page per folder (a subfolder pushes the next; the title's menu holds Enter folder path, Parent folder and the ancestors). Android and the desktop keep the same order in one scrolling sheet, Cancel and Use this folder in the footer.

With no recent selections, Browse other folders is the first action below the computer context. The empty Recent section and search stay hidden until there is history; filtering existing history to zero matches keeps search visible. The terminal promotes the Ctrl+B browse shortcut when history is empty.
