# Bot workspaces

New agent bots get a personal, persistent workspace automatically. Creating a bot does not require choosing a project. An omitted or empty `cwd` in `createBot` allocates `<CODYNC_HOME>/bots/<bot-id>/workspace` (normally `~/.codync/bots/<bot-id>/workspace`). Groups do not allocate folders. An omitted or empty `backend` gets Claude when it runs on the computer, else the first runnable agent Codync knows (the terminal UI relies on this; the apps still pick it themselves for older hosts), and omitted `connectors`/`skills` turn on every installed one.

The host persists the resolved absolute directory in `cwd` and exposes `managedWorkspace` in bot responses. Empty `cwd` in `updateBot` selects the same bot's personal workspace again. Existing explicit project directories remain unchanged, must already exist, and can still be selected in settings. Templates for personal bots send an empty cwd so a new bot gets its own directory.

## Task execution

The configured directory is the agent process and ACP session's starting directory. The bot's instructions explain that it can use explicit paths or change directory for an individual command when a task concerns another folder. This does not modify bot configuration or reset the conversation. Access remains subject to the harness's permissions; a personal folder is not a sandbox and does not grant additional access.

Routines and group contributions use their owner's configured starting directory. Task-specific paths should be explicit in routine instructions. The host does not infer a task directory from the last chat message. Harness-specific restrictions can still require user approval for other paths.

Changing the default project in settings deliberately reconfigures the agent session, as before. This is separate from executing a task in another directory.

## Lifecycle

- Renaming a bot or changing its model does not move or erase workspace files.
- Restarting the host or starting a new conversation context preserves the directory.
- Switching to a project and back reuses the personal workspace.
- Deleting a bot leaves its workspace files on disk. There is no automatic cleanup.
- Existing bots are not silently moved out of their configured project directories.

Swift settings offer Personal or Choose project folder; desktop quick creation and the terminal's new-bot flow default to Personal. Directory selection is optional. Separate workspaces do not prevent conflicts when multiple bots edit the same external project; automatic Git worktrees are not implemented here.
