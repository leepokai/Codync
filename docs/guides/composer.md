# Composer behavior and regression checks

The Electron composer restores the draft and layout behavior previously fixed in
`080b602` (per-conversation drafts), `4c7a26d` (animated field growth), and `f0abd80`
(Shift-Return). Drafts also survive restarting the client.

## Text and keyboard

- Desktop: Return sends; Shift-Return and Option/Alt-Return insert a newline.
  Confirming a CJK input-method candidate or holding Return does not send.
- The field grows and shrinks from one to eight lines, then scrolls internally.
  Window width and text-size changes remeasure wrapped text. The live textarea
  remains mounted during measurement, preserving selection and composition.
- The call, send, stop and interrupt actions share one button. Its symbol and
  colors transition without replacing the button or losing keyboard focus.
- Attachment rows, mention suggestions and the conversation inspector animate
  both opening and closing. Rapid toggles cancel pending reveal callbacks.
  Reduced Motion removes these transitions.
- iOS keeps its native multiline field, keyboard and existing Motion animations.
  Programmatic draft replacements clear the previous `TextSelection` before changing
  the string; iOS 18 can otherwise trap while converting stale indices to UTF-16.
  Keyboard edits bind directly to the local text and selection to preserve IME input.
  With a hardware keyboard (iPhone/iPad), Return inserts a newline at the cursor
  and ⌘↩ sends (send state only); on Android, Enter inserts a newline and
  Ctrl/Meta+Enter sends (send state only).
  The terminal uses immediate text layout, with Shift/Alt-Enter (where supported)
  or Ctrl+J for a newline; it has no voice button or graphical inspector animation.

## Attachments

iPhone and the desktop app show local image attachments as thumbnails above the text field,
with a remove button in the upper-right corner. The entire image fits inside the
preview; its original data remains the attachment sent to the host. On iPhone,
ImageIO creates display-sized thumbnails off the main actor; the desktop app
(`views/thread/ComposerAttachment.tsx`) lets the browser decode the image. Invalid image data falls
back to the filename chip, as do non-image files. Multiple attachments scroll
horizontally and remain removable individually.

On the iPhone a copied image goes in through **Paste** in the + menu; the
composer never suggests the clipboard on its own, and focusing it does not read
or attach clipboard contents. Picking a photo or image file uses the same preview.
A draft containing only attachments shows Send, including when another reply is
running (the message is queued).

The terminal UI keeps its filename row and backspace removal: its text renderer does
not implement terminal image protocols. Sending and limits: [file attachments](../features/file-attachments.md).

While replying to a message, the composer shows its quote on top: `text.bubble`, “text”
on one line and an X that cancels (the terminal UI: `↳ “…”`, esc cancels). The
placeholder reads "Reply…", and leaving the chat cancels the reply. See
[replies](../features/groups-and-replies.md).

## Draft lifetime

Text is saved exactly, including leading/trailing whitespace, blank lines and emoji.
Each chat has its own draft. Switching chats or reopening the client restores its
text. Deleting a bot removes its drafts.

Desktop stores text in a versioned localStorage record per account context and
computer, independently of transcript caches. iOS uses its account-scoped
SharedStore defaults with a separate map per computer. Signing out on iOS erases
these values with the rest of the account data. Retired desktop/iOS stores cannot
write late edits over a newly opened store.

The terminal saves text under `CODYNC_HOME` (normally `~/.codync`), in a private,
atomically replaced `tui-drafts-<URL hash>.json` file for each host endpoint.
Its existing send flow keeps a draft until delivery succeeds and preserves newer
text if an earlier send finishes later. Save failures leave text in the session
and display an error. Desktop/iOS submission transfers the text to their existing
message/outbox flow and clears only that destination's draft.

Drafts are local to each client, not synchronized between devices. Attachment
bytes are not persisted across restarts.

## Verification

Run desktop type checking and unit tests, the iOS package tests, and host CI checks
as described in [development](development.md#component-checks). Regression tests
cover exact-text restoration, independent destinations, selective clearing,
retired stores, and send acknowledgements arriving after new text is entered.

For a host-free visual check using the actual Electron renderer components:

```sh
cd apps/desktop
node tools/composer-check/serve.mjs
# Open http://127.0.0.1:5198/tools/composer-check/
```

The fixture never connects to the host or sends to a real bot. Check typing,
Shift-Return, CJK composition, eight-line overflow, width changes, bot switching,
replies (Toggle reply), reload restoration, offline sending, call/send/stop/interrupt changes,
attachment removal, inspector opening/closing and Reduced Motion. The fixture's
sent-message log and drafts are separate from real account data.
