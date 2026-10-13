# Computer identity and list ordering

A computer's identity is its host signing key's `computerId`, not its display name,
address, Apple login or transport. Pairing and account discovery of the same host
reuse that ID; iOS keeps one store per ID. Different computers may have the same
name without being merged. Resetting the host's data/keys makes a new identity:
its old offline entry may need removing. Signing in with Apple and Google shares
this state only when both reach the same Codync account.

Desktop and iOS show one section per selected computer, or a flat list when only
one is selected. Drag a section heading to reorder it. Desktop also supports
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
the order; its bots follow when it's dropped. Cancelling a drag leaves the saved order unchanged. A tap folds or expands
the section; Move up/down accessibility actions are also available. The desktop
app supports heading drags and Alt+Up/Down on macOS, Linux and Windows.
The terminal UI connects to one host, so it has no cross-computer ordering.

The order never leaves the device: the cloud stores no computer order.

## Bot order

A computer's bots (and group chats) keep the order the user drags them into, on every
device: the host stores it as each bot's `position` (`reorderBots`, `host/src/hub/order.rs`)
and syncs it like any bot change. Pinned bots stay above the rest, and a bot moves only
among bots with the same pin state. Until a computer's bots are first arranged, every
position is 0 and they sort by recent activity; after that they stay put and a new bot
appears on top. Hidden bots keep their position for when they return.

- **iPhone:** touch and hold a row, then drag it. The roster is a `List` with its styling
  removed, so the system does the reordering: holding still opens the row's menu, and dragging
  the lifted row moves it, also once the menu shows, unless the finger goes into the menu. Lifted,
  the row is a rounded card: it fills while the system holds its drag item (`BotListView.lift`). A bot
  dropped among another computer's bots goes back. The drop saves the order (`BotListView.moveLines`,
  `BotStore.reorder`). VoiceOver: Move up / Move down. SwiftUI's own drag and drop can't do this:
  with a context menu on the row, a drag never starts on iPhone.
- **Desktop:** drag a row, or Alt+Up/Down on the selected bot (macOS, Linux, Windows).
- **Terminal UI:** `J` / `K` move the selected bot down / up.
