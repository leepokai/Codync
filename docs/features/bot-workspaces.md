# Bot workspaces

New agent bots get their own persistent folder automatically (**Own folder** in the apps). Creating a bot does not require choosing a project. An omitted or empty `cwd` in `createBot` allocates `<CODYNC_HOME>/bots/<bot-id>/workspace` (normally `~/.codync/bots/<bot-id>/workspace`). Groups do not allocate folders. An omitted or empty `backend` gets Claude when it runs on the computer, else the first runnable agent Codync knows (the terminal UI relies on this; the apps still pick it themselves for older hosts), and omitted `connectors`/`skills` turn on every installed one.

The host persists the resolved absolute directory in `cwd` and exposes `managedWorkspace` in bot responses. Empty `cwd` in `updateBot` selects the same bot's own folder again. Existing explicit project directories remain unchanged, must already exist, and can still be selected in settings. Templates for bots in their own folder send an empty cwd so a new bot gets its own directory.

## Task execution

The configured directory is the agent process and ACP session's starting directory. The bot's instructions explain that it can use explicit paths or change directory for an individual command when a task concerns another folder. This does not modify bot configuration or reset the conversation. Access remains subject to the harness's permissions; a bot's own folder is not a sandbox and does not grant additional access.

Routines and group contributions use their owner's configured starting directory. Task-specific paths should be explicit in routine instructions. The host does not infer a task directory from the last chat message. Harness-specific restrictions can still require user approval for other paths.

Changing the default project in settings deliberately reconfigures the agent session, as before. This is separate from executing a task in another directory.

## Lifecycle

- Renaming a bot or changing its model does not move or erase workspace files.
- Restarting the host or starting a new conversation context preserves the directory.
- Switching to a project and back reuses the bot's own folder.
- Deleting a bot leaves its workspace files on disk. There is no automatic cleanup.
- Existing bots are not silently moved out of their configured project directories.

Settings on every client offer **Own folder** or **Choose project folder…**; desktop quick creation and the terminal's new-bot flow default to Own folder. Directory selection is optional. Separate workspaces do not prevent conflicts when multiple bots edit the same external project.

## Versions

A bot's own folder is a git repository of its own, so what each turn did can be seen and undone (`agent/versions.rs`). A project folder is never touched: its history is the user's. Neither is a repository already in the bot's own folder (one the bot or the user made before versions): Codync keeps versions only in one it made, marked by a `codync-versions` file in its `.git`.

- **Made** when the bot's actor starts (a new bot, a host start: bots from before versions, git installed since) and when the bot switches back to its own folder: `git init`, the mark, a `.gitignore` for what tools fill again by themselves (`node_modules/`, `.venv/`, `__pycache__/`, `.cache/`, `target/`, `dist/`, `build/`, `.next/`, `.gradle/`, `.turbo/`, `.DS_Store`…) when the folder has none, and a first commit. The bot is emitted again so `gitRepo` shows. Its own repository even when an outer folder is one (a home folder kept in git): `add` stays inside.
- **Saved** before each turn that works there (`Before: <first line of what it was asked>`, past Codync's own notes such as the message id; waited for up to 10 seconds, then left to finish by itself) and after it (`<first line>`), when something changed: `git add -A` and a commit. What the bot staged itself is kept (a `git rm --cached` stays untracked), except what is left out below: that is unstaged first (`git reset -- <path>`). Git runs with no time limit and is never killed, not even when the host stops: a killed `git add` leaves its lock, and no version would be saved there again. Each bot or task saves in the order it asked (`Saves`: the next save waits for the last), so a turn's changes never land under the next turn's name. A task working in its bot's folder saves too; one in a worktree doesn't.
- **As Codync**: every command carries `user.name=Codync`, `commit.gpgsign=false`, no hooks (`core.hooksPath` to a folder that isn't there, `--no-verify`), no fsmonitor, whatever the user set globally. One save at a time in a folder; folders don't wait for each other.
- **Left out**: a new or changed file over 50 MB, staged by the bot or not (`:(exclude,literal)` pathspecs), so a download doesn't swell every version, and `uploads/`, the files sent to the bot (`chat::uploads`), some of which Codync deletes on purpose.
- **Skipped**, not forced, while git is at work there: `index.lock`, a merge, cherry-pick or rebase in progress (the bot's own git use). Each skip is logged and counted (`stage: busy`).
- **Without git** nothing changes: no repository, no versions; the git line in Settings says git isn't installed.
- The bot is told in its instructions (`chat::context`) that its folder keeps versions in git, so it can show or bring back an earlier file when the user asks. There is no restore UI.
- Tasks of a bot in its own folder never get worktrees (`taskWorktrees` is ignored there, and the clients hide its switch): the bot's work stays in one folder, and overwritten files can be brought back from a version.
- Events: `own_folder_versioned` when a folder becomes a repository, `own_folder_save_failed` with its `stage` (`init`/`busy`/`look`/`add`/`commit`).

## Workspace picker

The desktop, iPhone and Android picker starts with recent selections for the bot's computer. The host keeps the last 12 unique selections in SQLite (`recent_workspaces`); `selectWorkspace` validates the absolute folder path and its readability before recording it. Selection alone does not update a bot: existing settings save behavior applies. Browsing never adds history.

`listDirs` includes optional `recent` and `ancestors` arrays. Clients use absence of `recent` to detect older hosts and validate selection through `listDirs`. The browser supports home expansion (`~`, `~/`, `~\\`), parent navigation, breadcrumbs, filtering within the current folder, empty states and inline errors. An empty Windows drive-list path cannot be selected. Paths belong to the current bot's host; the computer label is informational, not a host-switch action.

The terminal picker uses Ctrl+R for recent workspaces, Ctrl+B to browse and Ctrl+L to enter a path. Right enters a highlighted folder, Left goes to its parent, Enter confirms the highlighted folder, and `.` uses the current folder. Selection errors keep the picker open.

With no recent selections, Browse other folders is the first action below the computer context. The empty Recent section and search stay hidden until there is history; filtering existing history to zero matches keeps search visible. The terminal promotes the Ctrl+B browse shortcut when history is empty.
