# Memory as beliefs: "What I know about you"

Each **note** of a bot's memory (not the one-line facts of `MEMORY.md`) carries a header in its frontmatter, as plain text a
person can edit:

```
---
description: Where the user lives and works
type: user          # user | feedback | project | reference
confidence: high    # high | medium | low
confirmed: 2026-10-30
---
```

A note without the lines reads as a medium-confidence fact about the user, so every older note keeps working.

## The bot's tools (memory MCP server)

- `confirm_memory {file}`: the user said it still holds: high confidence, confirmed today.
- `correct_memory {file, content, summary?, question?}`: the whole corrected note; the header is kept, confidence goes to
  high and confirmed to today. The chat gets one line, **"I learned: <summary>"**, with an **Undo**. `question` is one short
  question about a contradiction; it is shown only if none was asked for this bot in the last 24 hours.
- `forget_memory {file, summary?}`: the note is taken out as a new version (a deletion). Its history keeps it, so it can be
  brought back. Also announced with an Undo.

Like every change of memory by a bot, these work only in a turn the user started in the bot's own chat.

## Host API

`beliefs {botId, kind?, confidence?, forgotten?}` lists the notes with their header, confirmation and versions (with
`forgotten: true`, the notes that were forgotten). `beliefAction {botId, file, shared, action}` with `confirm`, `forget` or
`restore` (+ `versionId`). `undoMemoryChange {botId, entryId}` takes back what an "I learned" line told about. Nothing is
destroyed for good: versions are kept as the memory history always keeps them.

## Screens

| Client | Where |
|---|---|
| Desktop | Memory → **What I know**: kind and confidence filters, Still true, Forget, History with Bring back, Show forgotten; Undo on the "I learned" line |
| Android | the same tab in the memory card; Undo on the line |
| Terminal | **What this bot knows about you…** in the command list (`y` confirm, `x` forget, `f`/`c` filters, `g` forgotten, ↵ history, `b` bring back). The "I learned" line shows, but its Undo is a button only on desktop and Android: in the terminal, bring the earlier version back from the history |
| iPhone | not done (no iOS work was asked for) |

The list shows the least sure first, then the longest unconfirmed.

## Not done

- Per-fact metadata for the one-line facts of `MEMORY.md` (the header is per note).
- The bot is not told when to ask; `correct_memory`'s `question` is the only door to the 24-hour rule.
