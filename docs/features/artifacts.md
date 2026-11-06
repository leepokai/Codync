# Artifacts and the Library

An artifact is a self-contained HTML document a bot makes for the user to read as a whole or come back to: a report,
a plan, a comparison, a tracker, a dashboard (Grok Bot's Pages, on the user's own computer). The bot writes the HTML;
the host keeps it; every app shows it as a card in the chat and opens it full size. A chat's **Library** (Grok Bot's)
holds everything made or shared there: its artifacts, the files and images the bot shared, and the ones the user sent.
Asked to change an artifact, the bot publishes it again under the same id, and every open copy shows the new one.

## Bots

The built-in `chat` MCP server has two tools (`host/src/chat/artifacts.rs`):

- `publish_artifact {path, title?, artifact?}`: the HTML file (absolute or relative to the bot's working directory,
  up to 5 MiB, inline CSS and JS, libraries from a CDN allowed). Without `artifact` it makes a new artifact and needs
  a title; with `artifact` (an id) it replaces that artifact's HTML, and its title when given. The answer names the
  artifact's id.
- `list_artifacts {}`: the bot's artifacts, newest first, with each one's id, title, last publish time and the path of
  its current HTML. To make changes the bot edits a copy and publishes that: the stored file is the live one, checked
  against its size and SHA-256 on every read.

When to make one is part of every bot's instructions (`chat::outbox::INSTRUCTIONS`): an artifact instead of a long
message, one artifact per subject, updated rather than made again. A session whose frozen instructions predate
artifacts gets a one-line reminder of this with each message in the bot's own chat (`turn.rs`, like `send_file`'s).

A publish in the bot's own chat also posts an artifact card there: a final `agent` entry with
`{text: "Artifact: <title>", final: true, artifact: {id, title, updatedAt}}`. Older apps show the text. In a routine
run, a group chat or a request from another bot the artifact is saved and in the chat's Library, but no card is
posted (`card` in the answer says so), and the bot mentions it in its reply.

A message about an artifact is a reply to its card: `replyTo` carries the artifact id and quotes it under its current
title (`replies::quote`), and the bot reads `[Codync: this is about your artifact "<title>" (id …). Its current HTML is
<path>; …]` instead of the quoted text (`artifacts::note`).

## Host

- Storage: `~/.codync/bots/<bot>/artifacts/<id>.html` holds the newest HTML (replaced whole: written to `.partial`,
  then renamed). The details live in kv as `artifact.<bot>.<id>`:
  `{id, botId, title, size, sha256, createdAt, updatedAt, entryId?, rev}`, where `entryId` is the newest card and
  `rev` the sync rev of the last change. Deleting the bot deletes both.
- API (Control access, like the chat):
  - `library {botId}` → `{items}`, newest first (`host/src/chat/library.rs`): the chat's artifacts
    `{kind: "artifact", at: updatedAt, artifact: Artifact}`, files its bot shared
    `{kind: "file", at, entryId, file: {id, name, size, sha256}}` (read with `readFile`) and files the user attached
    `{kind: "attachment", at, entryId, file: {id, name, size}}` (read with `readUpload`), from its newest 500 messages
    with files.
  - `readArtifact {botId, artifactId, offset}` → `{data: <base64>, size, artifact: Artifact}`, chunks of at most
    384 KiB like `readFile`.
  - `renameArtifact {botId, artifactId, title}` → `{artifact}`. Titles are trimmed, 1–200 bytes, no control
    characters. The newest card (`entryId`) takes the new title too, so older apps and a cache that missed the rename
    show it.
- Event: `{"type": "artifact", "rev", "artifact": Artifact}` on every publish and rename, stamped with a global rev
  like every other change, so the catch-up (`/events?since=`) replays the artifacts changed after the client's cursor.
  Apps upsert what the events bring and fetch the `library` when they show it; one they can't read is skipped
  (no rewind), and older apps ignore it.
- Additive: no `minApp` change. The apps show the Library only when the host's `hello` version is 2.25.0 or later
  (an older host has no `library`); cards need no check, since only those hosts post them.
- Analytics: `artifact_published {change, card}` and the `library` and `artifact` screens
  ([analytics events](../reference/analytics-events.md)).

## Apps

All four clients show the same things; layout follows each platform.

- **Card** (in the chat, for an `agent` entry with `artifact`): the top of the artifact itself, scaled down and not
  interactive, clipped to the card's corners, over a footer row with the artifact symbol (`chart.bar.doc.horizontal`),
  the title and "Artifact". It shows the newest version of that artifact (from `readArtifact`), whichever card it is.
  Tapping or clicking opens the viewer; the message menu still has Copy, Reply and reactions.
- **Viewer**: the artifact full size in a web view of its own (scripts on, no access to the app, storage of its own; a
  tapped web or mail link opens in the system browser, and other schemes go nowhere; the artifact sending itself or a
  new window elsewhere without a tap goes nowhere, and a card opens nothing. The desktop app checks the tap in its main
  process: a click or Return/Space in the window within a second, after the viewer loaded, opens one link; a
  touchscreen tap in the artifact never reaches the main process, so it opens nothing).
  - Header: close (`xmark`, "Close artifact"), the title in the middle (tapping it renames the artifact: "Rename
    artifact", "Artifact title"), and a share action: desktop "Save as HTML" and "Open in browser"; iPhone and Android
    the system share sheet with `<title>.html`.
  - Bottom: a message box, "Message about “<title>”", that sends a reply to the artifact's card (`entryId`) to its
    bot. It keeps a draft of its own (`<bot>/artifact/<id>`), apart from the chat's. The viewer stays open; when the bot publishes the artifact again, it reloads. An artifact without a card
    (published in a routine run) has no message box: there is nothing to reply to.
  - Loading shows the shared spinner; a failure shows "Couldn't open the artifact" with the error.
- **Library**: the chat's artifacts and the files shared in it, grouped Today / Yesterday / This week (the last
  seven days) / Earlier. An artifact row is the artifact symbol and its title, opens the viewer, and its menu has
  Open, Rename artifact and the share action. A file the bot shared is its file symbol, name and size, and saves like
  its card in the chat. An image the user sent shows a thumbnail and opens full size; other files they sent show their
  name and size. Empty: "Library is empty" / "Artifacts {name} makes and files shared in this chat show here."
  - Desktop: a Library tab in the conversation details panel (Details · Library · Memory · Settings).
  - iPhone and Android: the same tab in the bot's details sheet, which tapping the chat's title opens.
- **Terminal UI**: a card is a `▤ Artifact: <title>` line. On the selected message, `o` opens the artifact in the
  computer's browser (its HTML written to a temporary file) and `f` saves `<title>.html` to Downloads, like files.
  The Library is a tab of the bot's pages (`L`, ⇧← ⇧→): `↵`/`o` opens an artifact in the browser, `f` saves any row to
  Downloads, `r` renames an artifact.

Desktop: an artifact loads from `codync-artifact://<id>` (`apps/desktop/src/main/artifacts.ts`), its own origin, into
a sandboxed frame: the chat window's content security policy would block an artifact's scripts in a `srcdoc` frame.
What it opens goes to the browser by the Viewer's rule (`https`, `http` and `mailto` only): from the viewer only, after
a click or Return/Space; cards open nothing.

Not yet: public links (Grok Bot's "Anyone with the link"), deleting or pinning an artifact, artifacts in group chats.
