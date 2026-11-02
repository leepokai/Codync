---
name: codync-export
description: Pack the user's Grok Bot agents and group chats into one .zip file that Codync imports as bots, with their instructions, avatars, skills, scheduled tasks, memories and (if the user wants) workspace files. Use when the user asks to export, move or migrate their agents to Codync.
---

# Export agents for Codync

You write one `.zip` file. The user opens it in Codync (**+ → Import bots…** in the desktop app, or
`codync-host import <file>` in a terminal) on the same computer. Codync reads it there; nothing is uploaded.
Codync checks the manifest strictly, so keep to the format below.

## Steps

1. **Collect.** For every agent the user has, gather its name, a one-line role, its full instructions, its
   avatar image, its skills (each skill folder with `SKILL.md` and the files it uses) and its scheduled tasks
   (name, prompt, schedule, time zone, on or off). Read its chats only to write memories (step 3). Gather
   every group chat too: name, one-line topic, shared instructions, its agents (lead first) and its
   scheduled tasks.

   Leave out the agent running this skill and this skill itself (`codync-export`). If you can't read
   something, say so to the user later; never make up its contents.

2. **Ask.** Show the agents, the group chats and the size of the workspace files. Ask which agents and group
   chats to export (default: all) and "Include workspace files? (yes / no)". Wait for the answer.

3. **Memories.** From each agent's chats, write up to 50 facts it should keep: the user's preferences,
   names, recurring work, decisions, open tasks. One short statement per fact ("The user reports in EUR."),
   500 characters at most, never a copied message, never a secret. Up to 32 facts per group chat.

4. **No secrets.** Leave out `.env` files, keys, tokens, passwords and cookies, in files and in
   instructions; in instructions write `<ask the user for …>` instead. Codync skips files named `.env*` or
   `*private*key*`, nested archives (`.zip`, `.tar`, `.gz`, `.tgz`, `.7z`, `.rar`) and `.git` or
   `node_modules` folders, so don't include them.

5. **Write the folder** in the format below, `codync-import.json` last.

6. **Zip** the folder's contents (the manifest at the root of the zip) to
   `~/Downloads/.codync-export-<YYYY-MM-DD>.zip.part`, then rename it to
   `~/Downloads/codync-export-<YYYY-MM-DD>.zip` so Codync never sees half a file.

7. **Report** the file's path, how many agents and group chats it holds, and what you left out. Tell the
   user to open Codync's **+ → Import bots…** and choose the file.

## Format

```
codync-import.json
agents/<key>/avatar.png                 optional; PNG, JPEG or WebP, 5 MB at most
agents/<key>/skills/<skill>/SKILL.md    one folder per skill, with the files it uses
agents/<key>/files/…                    optional workspace files
```

`<key>` names one agent: lowercase letters, digits and hyphens, 64 characters at most, not starting or ending
with a hyphen (`research`, `sales-outbound`). Every path of an agent starts with `agents/<key>/`.

```json
{
  "format": "codync-bot-import",
  "version": 1,
  "source": { "app": "grok-bot", "exportedAt": "2026-10-31T10:00:00Z" },
  "agents": [
    {
      "key": "research",
      "name": "Research",
      "title": "Market research analyst",
      "description": "You find sources, compare competitors and write short cited briefs.",
      "avatar": "agents/research/avatar.png",
      "skills": ["agents/research/skills/web-brief"],
      "routines": [
        {
          "name": "Morning digest",
          "instruction": "Summarize yesterday's news about our competitors.",
          "active": true,
          "timezone": "Asia/Taipei",
          "schedule": { "kind": "weekdays", "time": "08:30" }
        }
      ],
      "memories": ["The user reports in EUR."],
      "files": "agents/research/files"
    }
  ],
  "channels": [
    {
      "key": "market-desk",
      "name": "Market desk",
      "title": "Weekly competitor review",
      "instructions": "Research brings the news; Sales says what it means for open deals.",
      "members": ["research", "sales"],
      "lead": "research",
      "routines": [],
      "memories": ["The user wants actions, not summaries."]
    }
  ]
}
```

- `name` (required, 80 characters at most) and `description` (required: the agent's full instructions) are
  text; `title` is optional. Very long instructions can go in `agents/<key>/files/INSTRUCTIONS.md` with a
  `description` that says to read `imported/INSTRUCTIONS.md`: Codync copies `files` to `imported/` in the
  bot's workspace.
- `avatar` and `files` are a path or `null`; `skills` lists skill folders, each with a `SKILL.md` that starts
  with front matter holding `name` and `description`.
- `routines`: up to 64 per agent; `timezone` is an IANA name. `memories`: plain strings.
- `channels` is optional. Each `members` entry is the `key` of an agent in this file; `lead` is one of them
  or `null`. A group chat's scheduled tasks and memories go to its lead in Codync.
- The whole zip stays under 500 MB and 5,000 files.

### Schedules

Times are 24-hour `HH:MM` in the routine's time zone; weekdays run from 0 (Sunday) to 6 (Saturday).

| Schedule | JSON |
| --- | --- |
| Hourly at minute 15 | `{ "kind": "hourly", "minute": 15 }` |
| Every day | `{ "kind": "daily", "time": "09:00" }` |
| Monday to Friday | `{ "kind": "weekdays", "time": "09:00" }` |
| Every week | `{ "kind": "weekly", "weekday": 1, "time": "09:00" }` |
| Every month | `{ "kind": "monthly", "day": 1, "time": "09:00" }` |
| Every N minutes, hours or days | `{ "kind": "interval", "amount": 30, "unit": "minutes" }` |
| Once | `{ "kind": "once", "date": "2026-12-01", "time": "09:00" }` |
| Anything else | `{ "kind": "custom", "expression": "0 9 * * 1-5" }` (five-field cron) |

Codync skips a routine whose schedule it can't read and tells the user; the agent still imports.

## Before zipping

- [ ] `codync-import.json` is at the zip's root, with `format` `codync-bot-import` and `version` `1`.
- [ ] Keys are unique and every path starts with `agents/<key>/`.
- [ ] Every skill folder has a `SKILL.md` with front matter; none is `codync-export`.
- [ ] The agent running this skill isn't in the file.
- [ ] Each group chat's `members` are agents in this file.
- [ ] No secrets; workspace files only if the user said yes.
