# File attachments

Files travel in both directions: users attach inputs, and bots explicitly share generated outputs.

- Composer: the **+** button left of the message box. iPhone: a menu with Photos (sent as JPEG) and
  Files (plus Paste when the clipboard has an image); desktop: an open panel. Files and images can
  also be dropped on the box or pasted: ⌘V / Ctrl+V on the desktop takes a copied image or file
  (`apps/desktop/src/renderer/views/thread/Composer.tsx`); on the iPhone through Paste in the
  + menu.
  HEIC/TIFF/BMP become JPEG (`OutgoingFile.prepared`, `Thread/Attachments.swift`). Picked files show as chips above
  the text and can be removed; a message can be files only. Group chats take files too: they're
  kept in the group's uploads folder and listed to each member under the message. Up to 100 MB per file.
- Upload: `upload {botId, uploadId, name, offset, data (base64), done}`, 384 KiB per call so each
  stays under the channel's 1 MiB message limit. A repeated chunk is accepted (retry); anything out
  of order is refused. Then `send {…, attachments: [uploadId]}`; the text may be empty.
- Storage (`host/src/chat/uploads.rs`): `<workspace>/uploads/<uploadId>/<name>` for a personal
  workspace (so reading needs no extra approval), otherwise
  `~/.codync/bots/<bot>/uploads/<uploadId>/<name>`. Only UUID ids and plain file names are accepted.
- The agent gets the message text plus an "Attached files" list of absolute paths and reads them with
  its own tools (Claude and Codex read images too). The entry keeps `attachments: [{id, name, size}]`
  for display.
- A queued message taken back (`unqueue`) deletes its uploads: the bot finds nothing of it in its folder, and
  `readUpload` refuses them. A message Stop dropped keeps its files, for the chat's *Not sent* row.
- Files don't go through the relay mailbox: while the computer is offline, a message with files
  waits in the composer's send button (disabled) instead. A failed send keeps its files on the device
  for Resend until the app quits. After that the iPhone and desktop refuse Resend with an error and the
  message stays failed, never sent as text alone (`filesGone`, `filesToResend`); Android saves the files
  with the pending row and the terminal keeps them in its draft.
- Chat: images show as pictures (tap for full size), other files as cards. Other devices fetch a
  sent file with `readUpload {botId, uploadId, offset}` (384 KiB chunks) and cache it; the sender
  caches its own copy at upload.
- Desktop opens an image in a large panel that fits the window; a large image shrinks to fit
  below the header without cropping, even when the window is resized, and a small one keeps its
  own size. Sent and bot-shared images use the same viewer.
- The desktop viewer zooms (trackpad pinch or ctrl + wheel, two fingers on a touch screen,
  double-click, `+` / `-` / `0`) and moves the picture by dragging or scrolling once zoomed in.
  The header has Copy image (the picture itself on the clipboard, re-encoded as PNG when it is
  another format, decoded straight to at most 36 megapixels so a huge one is never decoded whole) and, for a bot-shared file, the file's Download button next to the close button; while that file downloads or fails, its progress or error shows under the header. A Copy image from a picture's menu that fails shows "Couldn't copy image" in the app's error dialog.
  On every client a Copy image or Copy name started while an earlier Copy image is still fetching or converting takes its place:
  the earlier one leaves the clipboard alone and shows and reports nothing (`CopyRequests`). On desktop a Copy name
  also waits for a picture the clipboard is still writing, so the picture can't land after the name.
  Right-click on a picture, in the chat or the viewer, opens its message's own menu (reactions, Reply, what it did) with Copy turned into Copy image and Copy name. iPhone and
  Android open a picture in a sheet that zooms the same way (pinch, drag once zoomed in, double-tap
  to 2.5x and back; at fit size a swipe down still closes it), with Copy image (the picture
  itself; iPhone puts the file's own bytes on the pasteboard, plus a PNG copy when it is not PNG,
  JPEG or GIF, made off the main thread, upright, and scaled down past 4096 pixels on its long side
  like the viewer (`ImageData.largestPixels`), so a small compressed HEIC or TIFF can't run the phone
  out of memory; Android clips the original file as a FileProvider `content://` link) and, for a
  bot-shared file, its Download button by the close button. Holding a picture, in the chat or the
  viewer, opens that same message menu; in the Library it offers only Copy image and Copy name.
- Terminal UI: has no full-size viewer, zoom or Copy image: a terminal can't draw a picture, and the
  clipboard it reaches is text only. A picture shows as a file line like any other file, and `f` saves a
  bot-shared one, which is how the terminal downloads it. Dragging a file onto the terminal pastes its path, which becomes an attachment (⌫ on
  an empty draft removes the last one); sent files show as `▤ name size` lines. It uploads with the
  same `upload` chunks.
- A files-only message previews in the roster as its file names.

## Files from a bot

In its own chat, a bot calls the built-in chat MCP tool
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
in an undeleted bot's own chat. Existing caller permissions apply: paired
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

An image file (by extension) up to 20 MB also shows inline above its card on iPhone,
Android and desktop, tap/click for full size. A sound file (`mp3`, `m4a`, `wav`, `aac`, a
voice note) up to 20 MB gets a play/pause button in place of its icon: the host stamps it
`playsInChat` on the file's metadata (`SharedFile::plays_in_chat`, the one rule; the clients
only read the flag, and a file without it, from an older host, stays a plain card). For either the
client reads the whole file into memory through the same `readFile` chunks, checks size
and SHA-256, and keeps it in the sent-attachment cache under its file id; a failed image
read leaves the placeholder. While a sound loads its button shows a spinner; once loaded
its caption reads elapsed / total time over a progress bar. Tapping or dragging the bar
(clicking on desktop) moves the sound there, playing or paused; its touch area runs past the
4 pt bar without taking layout (on desktop only to the thumb's 12 px height, so a click
elsewhere on the card or player, including beside the bar, plays or pauses), and VoiceOver/TalkBack adjust or the arrow keys move it five
seconds. A sound that can't be read or decoded, also one that breaks mid-play, goes back to
play and its caption reads "Couldn't play. Tap to retry." (click on desktop), in the app's
language, with no dialog; the next tap reads it again. On the phones, starting a sound pauses the one playing.
Screen readers get one element per card, with play, view and download as its actions; a
double tap runs the first. On desktop the card is one keyboard stop: Enter or Space does its
first action (play, view, else download), and its menu (right-click, Shift-F10 or
VoiceOver's Show menu) lists them all; while the menu is open the card's own keys do
nothing. The terminal UI shows only the card.

On iPhone, a note preserves an active call or dictation session. Once recording ends,
the next play resets the audio session to playback and releases it on pause, stop or
completion; a saved recording category alone does not mean the microphone is active.

### A voice note inside its message

`send_file {path, text}` sends a message that carries a voice note: one final agent entry
`{text, final: true, files: [meta], voiceNote: true}`. The host accepts `text` only with a
sound the chat plays (the extensions above, up to 20 MiB; `playsInChat`) and
checks it like `send_message` (nonempty, at most 32 KiB). The message counts as one the bot
sent: like `send_message`, the agent's own reply text then stays in the trace. iPhone,
Android and desktop show the player at the top of
the bubble on one centred row (play/pause with its spinner, the progress bar once the length is
known, then elapsed / length, or the size before the first play, the same "Couldn't play"
caption), its glyph on the text's edge, and the Markdown text under it; screen readers
get the player as one element, "Voice note, <caption>", with Play/Pause and Download as its
actions, and the text as its own. The terminal UI shows the file line, then the text. A
voice note sent without `text` keeps its own card. Older clients show the card alone.
Analytics: the host reports `voice_note_sent`, and each app `voice_note_played` (`played`, or
`failed` with the step that broke: `load` or `playback`) when a tap starts a note from its beginning; never the file or the text
([event catalog](../reference/analytics-events.md)).

This feature adds no previews of other file types, folder packaging, background
transfers, persistent resume or Watch UI. Existing bots do not publish workspace files
automatically: they must call `send_file`.
