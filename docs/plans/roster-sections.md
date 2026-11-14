# Roster sections

User-made sections in the bot roster, like Slack's custom sidebar sections: "Work", "Personal",
each folding open and shut. Every client (desktop, iPhone, Android, terminal) gets them in the
same change. "Folder" is not used for this anywhere: it already means a bot's project folder
(`folder`, "Choose project folder…").

## What the user sees

```
  ★ Assistant               the lead bot: always first, outside sections
  📌 Reviewer               pinned rows next, also outside sections
▾ Personal (2)
    🤖 Notes Bot
    🤖 Mail Bot
▸ Work (3)  ●4              folded: the unread count of everything inside
Bots                        a plain label (no fold, count or menu), only when there are sections
  🤖 Scratch Bot            rows without a section
```

- A bot or group chat is in at most one section, or none.
- **Order:** the lead bot, then pinned rows, then sections in device-local drag order (initially sorted by name, the way the device's
  language sorts, so "İş" sorts as Turkish expects; the terminal sets case and accents aside), then
  rows without a section initially come last, under a "Bots" label that can also move. Inside a section rows keep
  the roster order (position, then recent activity).
- **Row menu:** "Move to section ▸" lists the existing sections, "New section…" and "Remove from
  section"; the lead bot has none. **Header menu:** "Rename section…" and "Remove section" (its
  bots stay, without a section).
- A section exists while a bot is in it: moving its last bot out removes the header.
- A pinned bot keeps its section and shows in it again once unpinned.
- Tapping a header folds or unfolds it (animated, `Motion`). Folded state is per device, not
  synced; renaming or removing a folded section on this device takes its fold along.
- Dragging moves a bot only within its band: the lead can't be dragged and nothing drops above it;
  pinned bots reorder among all pinned bots (pinned rows show no sections); an unpinned bot also
  stays in its section.
- Moves and renames show at once; one the host refuses is put back.
- Hidden bots keep their section.

Not in this change: dropping a bot onto a section by drag, nested
sections.

## Host

- `BotConfig` (`host/src/store/model.rs`) gains `section: Option<String>`
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`). The config is stored as one JSON
  blob, so there is no schema migration; `bot_json` carries it in `bot` events.
- `updateBot {id, section}`: a string sets it, `null` clears it. `Hub::update_bot` merges the patch,
  so clients that don't send `section` (older apps) leave it alone. The name is trimmed; an empty
  result clears the section; over 40 characters or containing a control character is refused.
  Validation runs for groups too (today `validate()` returns early for them).
- New method `renameSection {from, to}` (`host/src/api/bots.rs`, hub logic
  in `hub/sections.rs`): moves every bot of this computer in `from` to `to` under one
  `emit_lock`, then emits each changed bot. `to: null` removes the section. Renaming onto an
  existing name merges the two. `to` is validated like `updateBot`'s `section`.
- Bots can't set sections: `section` stays out of `chat/manage.rs` `FIELDS`.
- Analytics: `bot_updated` lists `section` among changed fields; `renameSection` gets an event
  (`analytics/observe.rs`, `docs/reference/analytics-events.md`). Names are never sent.
- Compatibility: additive (a new optional field and a new method), so `MIN_APP` / `MIN_HOST` stay.
  Clients show the section controls only when the host's version (from `hello`) is at least
  2.32.0, the release that ships this.

## Clients

Each client gets one pure function that turns the sorted roster into lines (the lead and pinned
rows, section headers with their rows, the "Bots" label, rows without a section) and is
unit-tested. The drag guards compare the band (lead, pinned, the rest), and for unpinned bots the
section too.

| Client | Model | Header | Menus | Name entry | Folded state |
|---|---|---|---|---|---|
| Desktop | `shared/models.ts` `Bot.section`, `normalizeBot` | modeled on `ComputerRosterSection` | `views/DesktopActionMenu.tsx`, items from `views/section-menus.ts` (Move to section as a hover or keyboard submenu) | `Dialog` | `prefs.ts`, `collapsedRosterSections` |
| iPhone | `Bot.swift` `Bot.section: String?`; `HostClient.setSection` sends `null` to clear | modeled on `ComputerSection` | system `contextMenu` submenu (`.contextActions`) | `.codyncDialog` | `@AppStorage("collapsedRosterSections")` |
| Android | `HostClient.kt` `Bot.section`; `BotDraft.template()` leaves it out | modeled on iOS `ComputerSection` (`RosterSectionViews.kt`) | `contextActions` | `CodyncDialog` | `SharedPreferences` |
| Terminal | `tui/app/bot.rs` `Bot.section` | labels beside `PINNED` / `BOTS` in `view/roster.rs` | keys, listed in `view/help.rs` | `section_fields` sheet (`manage/fields.rs`) | in memory |

Desktop drag guards: `makeRoom` (`BotRoster.tsx`) and `moveBot` (`store/computer-roster.ts`), both
through `sameRosterGroup` (`lib/roster-sections.ts`); iPhone `BotStore.move` and `BotStore.drop`; Android `moved()`; terminal `reorder_bot`. The desktop demo host
(`demo/fake-host.ts`) handles `renameSection`.

## Tests

- Host: set and clear; trimming, length and control-character refusal; groups validated;
  `renameSection` rename, merge and remove; re-record `tests/fixtures/wire-shape.json`
  (`CODYNC_UPDATE_WIRE_SHAPE=1 cargo test --test e2e wire_shape`).
- Desktop: the grouping function and the drag guard (`src/renderer/lib/*.test.ts`).
- iPhone: `RosterOrderTests`. Android: `RosterMoveTest` plus grouping. Terminal: `navigate.rs`.
- End to end on each client: create a section, move bots in, fold it with an unread message
  inside, rename it onto another section's name, remove it.

## Release

This change bumps every version file to 2.32.0, the first host with sections, and clients gate the
section controls on a 2.32.0 host. Its What's New bullets are in the `## 2.32.0` sections of
`apps/ios/WhatsNew.md` and `apps/android/WhatsNew.md`. [Bot order](../features/computer-order.md#bot-order)
and the scope table in [remote relay](../reference/remote-relay.md) cover `renameSection`.

Section headings and the unsectioned Bots group can be reordered on each device, independently per computer. A rename keeps its place; a merge keeps the destination's place. Pinned bots stay on top. Terminal Alt+↑/↓ moves the selected bot's group for that terminal session.
