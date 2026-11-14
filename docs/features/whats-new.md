# What's new

A release the maintainer calls important gets an announcement: the first time each app opens on that version (or a
later one), it shows the release's highlights once, in a **What's new** window over its main screen. Every other
release shows nothing. The iPhone, Android and desktop apps and the terminal UI all show it.

## Announcing a release

Only when the maintainer says a release's change is important (any wording: 重要, announce, show it on update),
add its entry to `apps/ios/Kit/Sources/CodyncKit/Resources/Announcements/announcements.json` in the change that bumps
the version, newest first:

```json
[
  {
    "version": "2.44.0",
    "notes": [
      "New: Codync Cloud (beta). Start a cloud computer on your own Boat or InstaBox account, so your bots work while your computer is off.",
      "Fixed: menus sometimes ignored taps while a bot updated."
    ]
  }
]
```

- `version` is the version the change first ships in; any later version shows it too, to devices updating from
  below it.
- Each note is an English interface phrase, one or two sentences of what people notice and where to find it, the
  same facts as the store notes and without emoji. Add every note to every catalog in
  `apps/ios/Kit/Sources/CodyncKit/Resources/Languages/` ([interface languages](../guides/localization.md)); the
  desktop catalog test and the iPhone kit's `WhatsNewTests` fail on a missing translation.
- Keep one announcement: when a newer one is added, delete the older entry and its phrases from the catalogs. A
  device that skipped several sees only the newest.

Every client reads this one file: the iPhone bundles it from CodyncKit, Android adds the folder as an asset source,
the desktop app imports it and the terminal UI embeds it.

## When it shows

Each device keeps the version it last opened on (`seen`):

- At launch, the announcements with `seen < version ≤ current` are due, newest first.
- Without `seen`, a new install records the current version and shows nothing. A device set up before this feature
  (the phones' onboarding finished, the desktop app past its welcome) counts as `0.0.0`, so its first update after
  this feature can show an announcement. The terminal UI has no such sign and starts like a new install.
- Nothing due: the current version is recorded at once. Otherwise the window opens over the main screen, never over
  the welcome, sign-in or pairing screens, and the current version is recorded once it is on screen. It only closes
  (swipe down or ✕ on the phones, ✕ or Esc on the desktop, Esc in the terminal).
- Going back to an older version shows nothing and records that version.

| Client | Window | Saved as |
|---|---|---|
| iPhone | `WhatsNewSheet` (`CodyncUI`), a system sheet over the tabs | UserDefaults `whatsNewSeen` |
| Android | `WhatsNewSheet`, the same sheet | `whatsNewSeen` preference |
| Desktop | `WhatsNewSheet`, a `Sheet` over the chat window | pref `whatsNewSeen` |
| Terminal UI | `Overlay::WhatsNew`, a sheet at start | choice file `tui-whats-new` |

## Analytics

`whats_new_shown` (phones directly, desktop and terminal UI through the host's `track`), when the window is on screen;
`app_version` says which release. See [analytics events](../reference/analytics-events.md).

## Testing

Each client unit-tests which announcements are due. To see the window end to end, add a temporary entry for the
current version with notes from the catalogs (an existing phrase works), set the device's `seen` to an older version
and launch; remove the entry afterwards.
