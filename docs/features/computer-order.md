# Computer identity and list ordering

A computer's identity is its host signing key's `computerId`, not its display name,
address, Apple login or transport. Pairing and account discovery of the same host
reuse that ID; iOS keeps one store per ID. Different computers may have the same
name without being merged. Resetting the host's data/keys makes a new identity:
its old offline entry may need removing. Signing in with Apple and Google shares
this state only when both reach the same Codync account.

Desktop and iOS show one section per selected computer, or no computer headings when
only one is selected. Drag a section heading to reorder it. Desktop also supports
Alt+Up/Down on the heading; iOS exposes Move up/down accessibility actions.
Filtering, folding and ordering are device-local preferences. The iPhone and each
desktop installation keep their own order, even when signed in to the same account.

## Local persistence

Desktop saves the order in localStorage, separately for each account and cloud
environment. Signed-out ordering has its own local preference. iOS saves the order
in its App Group UserDefaults, scoped to the local account context. Existing cached
positions remain in place; there are no uploads, downloads, pending writes or polls.
Reordering works offline and survives restarting the app.

Devices show their available computers in their own saved order and append newly
available computers. Missing/filtered computers retain their position for when
they return. Computer names and bots never become ordering keys, and an ordering
entry never grants access to a computer or imports another device's SSH connection.

On iOS, touch and hold a computer heading, then drag up or down and release to save
the order. The computer moves with its bots: while the heading is lifted, every computer's
bots fold into their headings (rows kept at no height, so the system's drag keeps its place),
and they unfold under their computers when it's dropped. Cancelling a drag leaves the saved order unchanged. A tap folds or expands
the section; Move up/down accessibility actions are also available. The desktop
app drags headings the same way (the sections fold while one is lifted, and the others make
room as it passes them) and supports Alt+Up/Down, on macOS, Linux and Windows.
The terminal UI connects to one host, so it has no cross-computer ordering.

The order never leaves the device: the cloud stores no computer order.

## Bot order

A computer's bots (and group chats) keep the order the user drags them into, on every
device: the host stores it as each bot's `position` (`reorderBots`, `host/src/hub/order.rs`)
and syncs it like any bot change. Every client sorts in bands: the [lead bot](lead-bot.md)
first (it can't be dragged, and nothing drops above it), then pinned bots, then the rest;
a bot moves only within its band, and an unpinned bot also stays in its section (the lead and
pinned rows show no sections, so pinned bots reorder among all pinned bots). Until a computer's
bots are first arranged, every position is 0 and they sort by recent activity; after that they
stay put and a new bot appears at the top of its band. Hidden bots keep their position for when
they return.

A bot or group chat can be in one section, a name the user picks (`section`, set with `updateBot`;
`renameSection` renames, merges into another section's name, or removes one; `host/src/hub/sections.rs`).
The host trims the name, treats an empty one as none, and refuses one over 40 characters or with a
control character. The roster shows the lead and pinned rows first, then each section under its header
in device-local order, initially sorted by name (the way the device's language sorts; the terminal UI sets case and accents aside but
has no per-language rules, so Swedish "å" sorts as "a" there), then rows in no section (under a "Bots"
label when there are sections); inside a section rows keep the order above. A section exists while
a bot is in it. A bot's menu has Move to section and a section header's menu has Rename section and
Remove section, offered only for a host at 2.32.0 or newer; the apps show the change at once and put
it back when the host refuses it. The lead stays on top, outside sections, and its menus offer no
Move to section. A
pinned bot keeps its section and shows in it again once unpinned; hidden bots keep
theirs. Each device folds sections for itself, by name: renaming or removing a folded section on
this device takes its fold along (a merge folds the section it joins; an unfolded section changes
nothing), while a rename made on another device arrives unfolded.

- **iPhone:** touch and hold a row, then drag it. The roster is a `List` with its styling
  removed, so the system does the reordering: holding still opens the row's menu, and dragging
  the lifted row moves it, also once the menu shows, unless the finger goes into the menu. Lifted,
  the row is a rounded card: it fills while the system holds its drag item (`BotListView.lift`). A bot
  moves only among its own computer's bots with the same pin state and, unpinned, the same section:
  dropped among others (another section, another computer's), it goes first or last of its own. The
  drop saves the order (`BotListView.moveLines`, `BotStore.drop`). Dragging a section heading or the Bots label moves that whole group within its computer; each device keeps this order.
  VoiceOver: Move up / Move down. SwiftUI's own drag and drop can't do this:
  with a context menu on the row, a drag never starts on iPhone.
- **Desktop:** drag a row, or Alt+Up/Down on the selected bot (macOS, Linux, Windows). Drag section headings or the Bots label to move whole groups; Alt+Up/Down on a focused heading does the same.
- **Terminal UI:** `J` / `K` move the selected bot down / up. `w` moves it to a section (empty takes
  it out), `W` renames its section (empty removes it), `z` or a click on a section's label folds or
  unfolds it (in the compact strip a click on a folded section unfolds it). Alt+Up/Down moves the selected bot's whole section, including Bots; section order lasts until quitting.
