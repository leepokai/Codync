# File attachments

Files travel in both directions: users attach inputs, and bots explicitly share generated outputs.

- Composer: the **+** button left of the message box. iPhone: a menu with Photos (sent as JPEG) and
  Files (plus Paste when the clipboard has an image); desktop: an open panel. Files and images can
  also be dropped on the box or pasted: ⌘V / Ctrl+V on the desktop takes a copied image or file
  (`apps/desktop/src/renderer/views/thread/Composer.tsx`); on the iPhone through Paste in the
  + menu.
  HEIC/TIFF/BMP become JPEG (`OutgoingFile.prepared`, `Thread/Attachments.swift`). Picked files show as chips above
  the text and can be removed; a message can be files only. Not in group chats (a group has no
  folder of its own). Up to 100 MB per file.
- Upload: `upload {botId, uploadId, name, offset, data (base64), done}`, 384 KiB per call so each
  stays under the channel's 1 MiB message limit. A repeated chunk is accepted (retry); anything out
  of order is refused. Then `send {…, attachments: [uploadId]}`; the text may be empty.
- Storage (`host/src/chat/uploads.rs`): `<workspace>/uploads/<uploadId>/<name>` for a personal
  workspace (so reading needs no extra approval), otherwise
  `~/.codync/bots/<bot>/uploads/<uploadId>/<name>`. Only UUID ids and plain file names are accepted.
- The agent gets the message text plus an "Attached files" list of absolute paths and reads them with
  its own tools (Claude and Codex read images too). The entry keeps `attachments: [{id, name, size}]`
  for display.
- Files don't go through the relay mailbox: while the computer is offline, a message with files
  waits in the composer's send button (disabled) instead. A failed send keeps its files on the device
  for Resend until the app quits.
- Chat: images show as pictures (tap for full size), other files as cards. Other devices fetch a
  sent file with `readUpload {botId, uploadId, offset}` (384 KiB chunks) and cache it; the sender
  caches its own copy at upload.
- Terminal UI: dragging a file onto the terminal pastes its path, which becomes an attachment (⌫ on
  an empty draft removes the last one); sent files show as `▤ name size` lines. It uploads with the
  same `upload` chunks.
- A files-only message previews in the roster as its file names.

## Files from a bot

In its own chat or a reply thread, a bot calls the built-in chat MCP tool
`send_file {path, name?}`. Paths may be absolute or relative to its working directory.
The optional name is a display filename without folders. Any regular file type is
accepted, including binary, hidden, extensionless and empty files, up to 100 MiB.
Directories and special files are refused. Group, routine and delegated turns cannot
publish files.

The host streams a snapshot into `~/.codync/bots/<bot>/files/<fileId>` and records
its byte size and SHA-256 before publishing a final agent entry with
`files: [{id, name, size, sha256}]`. Copying runs outside the bot actor; Stop,
reconfiguration, timeout and turn completion cancel unpublished copies. A success
response means the file message is already saved. Check the conversation before
retrying an interrupted publication, to avoid duplicate messages.

Snapshots remain available after the original is edited, removed, or its workspace
changes, and after a host restart. Storage follows the host data directory rather
than the bot working directory. Generated files are separate from input uploads;
`attachments` and `readUpload` retain their existing meaning. Older clients show
the entry's filename/size text fallback. No version or compatibility floor changes.

The authenticated `readFile {entryId, fileId, offset}` API requires a nonnegative
integer offset within the saved size and a file belonging to that final agent entry
in an undeleted bot's own chat or thread. Existing caller permissions apply: paired
clients need Control access. It returns `{data: <base64>, size}` in chunks of at most
384 KiB over loopback, SSH, direct encrypted and relay transports.

Updated clients show a filename, size, file icon and download action. File extension
only selects the icon; it never restricts saving or changes bytes. Desktop uses its
native Save dialog, iPhone hands the verified temporary file to the native share
sheet (including Save to Files), and the terminal's selected-message `f` action saves
to Downloads without replacing existing files. Progress and errors appear alongside
the action; cancel stops further reads, discards late responses and removes partial
files. Retry starts from zero. Account/computer retirement cancels its transfers.
Downloads use bounded memory and check size and SHA-256 before save/export.

This feature adds no previews, inline viewers, folder packaging, background transfers,
persistent resume, Android or Watch UI. Existing bots do not publish workspace files
automatically: they must call `send_file`.
