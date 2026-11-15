# Share to a bot

Other apps' share sheets send text, links, pictures and files to a bot, on iPhone and Android. User manual:
[sending messages](../../web/content/docs/conversations.mdx).

## The flow (both phones)

1. In any app: Share → **Codync**.
2. The **Send to a bot** sheet, top to bottom: the shared files (thumbnail or file icon, name, size), a message field
   (*Add a message*, holding the shared text and links, editable), a status line when there is one, **Recent**, then
   each computer's bots in roster order (the computer's name over each when the account has more than one, else
   *Bots*). Hidden bots and groups with no members aren't listed. With no computer: *Open Codync and connect a computer
   first.*; with no bots: *No bots yet*.
3. Tapping a bot picks it (a check on its row); Send (`arrow.up` in the header) sends the message with its files.
4. Done: the iPhone sheet closes, back in the app that shared; Android opens the chat.

The share sheet sends rather than putting the share in a chat's message box: an iPhone share extension has no supported
way to open its app (the responder-chain `open` trick is unsupported and can fail App Review), so both phones do the
same thing. The chat's own draft is never touched.

### Recent: whom it went to last

The last three chats shared to are kept per account, most recent first (`ShareRecipient`), and updated when a share is
handed over (sent or queued, never on failure). They head the list as **Recent** (with the computer's name under each
when there's more than one computer) and are left out of the computer sections below; the most recent one that's still
listed is picked when the sheet opens. iPhone: `SharedStore.Context.shareRecipients` (App Group, erased with the
account); Android: `share-recipients.json` in the account's folder (`LocalStore`).

### Rules

- Up to 10 items, each file up to 100 MB (`OutgoingFile.maxSize`): a bigger one shows *Larger than 100 MB* on its row
  and stays behind. On the iPhone, a file it couldn't read shows *Couldn't read this file* and stays behind too; Android
  reads the files on Send, and one it can't read stops the share with the reason. HEIC/TIFF/BMP become JPEG, as in the
  composer (`OutgoingFile.prepared`). The iPhone offers Codync only for 10 or fewer (`NSExtensionActivationRule`);
  Android takes the first 10.
- A link is a line of the message, not a file. Text from an app that also names it as a file (a `.txt` from Files) is a
  file.
- Text and links that came in are sent as `pasted` (both phones, the relay mailbox included) while the message still
  holds them, so [memory](context-and-memory.md) never takes them as the user's own words; once edited or deleted they
  aren't sent at all (`IncomingShare.pasted`, Android `SharedContent.pasted`).
- Every Send of one share uses the same `clientNonce`, so a retry after a lost reply never delivers it twice.
- Computer offline: text alone is queued like any message; with files, *Files go out once your computer is online.*
- Analytics: `share_sent` ([event catalog](../reference/analytics-events.md)).

## iPhone

The `Share` app extension (`apps/ios/Share/`, `com.apple.share-services`, bundle id `<app id>.Share`) links CodyncKit
only, like the widgets and the notification extension, with the App Group and Keychain group:

- `SharedItems`: reads the item providers; what becomes a line of the message or a file, and a file's name, is decided
  by `IncomingShare` (CodyncKit, tested in `ShareTargetsTests`). Files are copied into the extension's temporary folder
  (a provider's file lasts only for its callback) and read `.mappedIfSafe`, so a 100 MB file never sits in the
  extension's ~120 MB of memory; the folder goes when the sheet closes.
- `ShareModel` / `ShareSheet`: the sheet, from `ShareTargets` (CodyncKit: Recent, then each computer in the user's
  order, from the bots the app mirrors into the App Group).
- `ShareDelivery`: connects like the app (`HostConnector.connect` with the device key from the shared Keychain),
  uploads the files with `HostClient.upload(botId:files:)` (the app's own: fresh upload ids on every attempt; the bar
  under the message follows the bytes) and `send`s; offline (relay presence), text goes to the relay mailbox
  (`ChannelTransport.enqueue`). A computer in no account has no relay, so an offline one can't be reached from the
  sheet. Errors show under the message (*Can't reach your computer…*, *Couldn't send. Try again.*) and Send retries. A
  swipe down closes it, except while sending.
- Analytics: the extension has neither this install's ID nor the Settings switch, so it leaves `share_sent` in the App
  Group (`Analytics.queueShare`) and the app sends it on its next trip to the foreground. A file it couldn't read is
  reported once, as `failed` / `read`, when the sheet opens.
- `IconButton`, `ModalHeader` (`Design/ModalChrome.swift`), `CardForm`, `CardSection` (`Design/Cards.swift`),
  `Spinner`, `AttachmentIcon`, `OutgoingFile` and `ImageData` live in CodyncKit so the extension draws and prepares
  files like the app.
- The app shows the sent message through `/events` like one from another device, its files fetched with `readUpload`.

## Android

- `ShareActivity` takes `SEND` and `SEND_MULTIPLE` of any type and hands the text and the files (as `ClipData` with
  `FLAG_GRANT_READ_URI_PERMISSION`, since the sender's grant ends with `ShareActivity`) to `MainActivity`
  (`SharedContent.from` / `forward`, in `SharedContent.kt` with the sheet's other models). The files are the stream
  extra; the clip counts only without one, and never for a text share, whose clip is its link preview's thumbnail. Only
  `content://` URIs from other apps are taken: Codync's own provider is refused, since any app could otherwise make it
  send one of its private files.
- `ShareToBot.kt`: the sheet. Send imports the files and hands the message to the chat's pending sends
  (`AppStore.sendShared`: no draft involved; once saved, closing the sheet no longer stops it), then opens the chat,
  where upload and delivery show like a typed message. Send follows the composer's rule (online, or text alone while
  the relay or the phone can hold it: `AppState.holdsText`).
- `share_sent`: `sent` when handed over with the computer online, `queued` when it isn't; `error` is `read` (a file
  couldn't be copied in) or `send`.

## Not affected

- Host: no change (`upload` / `send` as before), no `minApp` / `minHost` change.
- Desktop: files already come in by drop, paste and the + panel; a macOS Share menu entry would need a native extension
  in the Electron app, and Linux and Windows have no common share target.
- Terminal UI: a terminal has no share target; dragging a file in attaches it.

## Not done

- Bots as share-sheet suggestions: the iPhone would donate outgoing `INSendMessageIntent` (conversation id
  `<computerId>/<botId>`, as `NotificationService` does for incoming ones) and list it in the extension's
  `IntentsSupported`; Android would push sharing shortcuts (`<share-target>`). Recent covers picking the usual chat.
- Sending to several bots at once.
