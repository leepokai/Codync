# Topic 15: demo mode

**Written, not compiled, not run** (the user asked for the gates to wait for one pass at the end).

## What changed

- Desktop: `store/demo-mode.ts`, `demo/demo-transport.ts`, `lib/memory-storage.ts`, `views/DemoBadge.tsx` (+ css),
  `views/settings/DemoSection.tsx`; `AppModel` takes an optional demo (its own transport and snapshot, no account or analytics
  link); `BotMirror` has a `volatile` flag (no cache, drafts in memory); `AnalyticsQueue.setQuiet`; `main.tsx` shows the demo's
  model in place of the real one; the story gained a routine. Test: `demo/demo-isolation.test.ts`.
- Android: `core/DemoScript.kt` (+ `DemoScriptTest`), `DemoMode.kt`, the toggle in `SettingsView`, the swap in `MainActivity`,
  the refusal in `AppStore.ready()`, the mute in `Analytics`.
- Terminal: `tui/demo.rs` (script and tests), `net::Client::demo()`, the second `App` in `tui/mod.rs`, the header badge, the
  roster preferences neither read nor written in the demo.

## Decisions

- The demo is a second model, not a swap of the data of the first: leaving restores *exactly*, because the first never stopped.
- The isolation tests read source files. They cannot see a run; they keep the demo's own files from importing the ways out.

## Not tested

Nothing compiled or run. No screen seen. iOS not done.
