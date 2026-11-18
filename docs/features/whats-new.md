# What's new

A release the maintainer calls important gets an announcement: the first time each app opens on that version (or a
later one), it shows the release's highlights once, in a **What's new** window over its main screen. Every other
release shows nothing. The iPhone, Android and desktop apps and the terminal UI all show it.

## Announcing a release

Only when the maintainer says a release's change is important (any wording: 重要, announce, show it on update),
add its entry to `apps/ios/Kit/Sources/CodyncUI/Resources/Announcements/announcements.json` in the change that bumps
the version, newest first:

```json
[
  {
    "version": "2.44.0",
    "features": [
      {
        "title": "Codync Cloud",
        "beta": true,
        "body": "Start a cloud computer on your own Boat or InstaBox account, so your bots work while your computer is off.",
        "media": "cloud",
        "film": "https://youtu.be/..."
      }
    ],
    "notes": ["Fixed: menus sometimes ignored taps while a bot updated."]
  }
]
```

- `version` is the version the change first ships in; any later version shows it too, to devices updating from
  below it.
- `features` are the highlights, each a story: its clip, then `title` (the feature's name as the interface writes
  it) and `body` (one or two sentences of what people notice and where to find it). `beta` adds the Beta tag.
  `notes` are smaller changes, text only, under **Also in this release**. An entry has at least one of the two.
- `media` names the feature's preview clip, `<media>.mp4` in the same folder, which ships inside every app (so it
  plays offline). Keep it small: about 9 seconds, 960×540, 30 fps, H.264 without an audio track, under 300 KB, cut
  from the feature's film in `videos/`:
  `ffmpeg -ss 4 -t 9 -i <film>.mp4 -an -vf scale=960:540,fps=30 -c:v libx264 -profile:v main -crf 28 -preset slow -pix_fmt yuv420p -movflags +faststart <media>.mp4`.
  Without `media` the story is text only.
- `film` is the address of the full video (with sound); without it there is no play button.
- Every `title`, `body` and note is an English interface phrase, the same facts as the store notes and without
  emoji. Add each to every catalog in `apps/ios/Kit/Sources/CodyncKit/Resources/Languages/`
  ([interface languages](../guides/localization.md)); the desktop catalog test and the iPhone kit's `WhatsNewTests`
  fail on a missing translation or a missing clip, and the desktop one also on a clip no feature names or one of
  300 KB or more.
- Keep one announcement: when a newer one is added, delete the older entry, its clips and its phrases that nothing
  else uses from the catalogs. A device that skipped several sees only the newest.

Every client reads this one folder: the iPhone bundles it from CodyncUI (the app only, not its extensions), Android adds it as an asset source, the
desktop app imports the file and its clips, and the terminal UI embeds the file.

## What it looks like

The window's header reads **What's new** with the version beside it. Under it the stories scroll in one column, no
pages: a 16:9 clip with rounded corners that plays muted in a loop while the window is open and the app is in front
(resting on its first frame when the system asks for reduced motion), the title with its Beta tag, and the body in the secondary color.
A round play button in the clip's lower right corner opens `film` in the browser (`link_opened` with `link: whats_new_film`).
The terminal UI can't play a clip: it shows each title and body, then a `▶` line with the film's address.

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
| iPhone | `WhatsNewSheet` (`CodyncUI`, clips by `LoopingClip`), a system sheet over the tabs | UserDefaults `whatsNewSeen` |
| Android | `WhatsNewSheet` (clips by `LoopingClip`), the same sheet | `whatsNewSeen` preference |
| Desktop | `WhatsNewSheet`, a `Sheet` over the chat window | pref `whatsNewSeen` |
| Terminal UI | `Overlay::WhatsNew`, a sheet at start | choice file `tui-whats-new` |

## Analytics

`whats_new_shown` (phones directly, desktop and terminal UI through the host's `track`), when the window is on screen;
`app_version` says which release. See [analytics events](../reference/analytics-events.md).

## Testing

Each client unit-tests which announcements are due. To see the window end to end, add a temporary entry for the
current version with phrases from the catalogs (existing ones work) and an existing clip, set the device's `seen` to an older version
and launch; remove the entry afterwards. The desktop's `e2e/whats-new.spec.ts` and the phones' Maestro flow
`apps/maestro/whats-new.yaml` (tag `whats-new`) walk the window for the announcement the file has. A clip the desktop
app can't load goes to Error tracking (`ClipFailed`, mechanism `whats-new-clip`).
