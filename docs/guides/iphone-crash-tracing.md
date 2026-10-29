# iPhone crash tracing

How an iPhone crash reaches PostHog with readable frames, how to read one, and what to do when
it isn't readable. What the reports may contain: [usage analytics](../features/analytics.md#error-tracking).
Why it is built this way: [error tracking detail](../plans/error-tracking-detail.md) (A1, A2).

## How a crash becomes readable

1. **On the iPhone.** MetricKit hands the app the previous launch's crash on the next launch.
   `CodyncKit` `CrashReports` sends it as a `$exception` (process `ios`) with PostHog `apple`
   frames: `instruction_addr`, `image_addr` (where the binary was loaded: address minus
   `offsetIntoBinaryTextSegment`), `module`, `in_app`, and `function` = `binary + offset` as the
   fallback name. The event-level `$debug_images` lists each binary once: its UUID (`debug_id`,
   uppercase) and load address. The event goes in the session the crashed launch ended in
   (`$session_id`), not the launch that reported it.
2. **At release.** After `Submit iOS`, the `iOS dSYMs` workflow (`.github/workflows/ios-dsyms.yml`,
   macOS) runs `tools/upload-dsyms.py v<version>`: it finds the Xcode Cloud "Release" run on the
   tag's commit, downloads the run's archive and uploads every `.dSYM` in it with
   `posthog-cli dsym upload`. Each dSYM is stored under its binary's UUID. This happens while
   the version waits for App Review, before the App Store releases it.
3. **In PostHog.** Ingestion matches each frame's `image_addr` to a `$debug_images` entry, loads
   the dSYM with that `debug_id` and looks up `instruction_addr − image_addr`: the frame gets its
   function, file and line.

Versions before 2.19.2 sent `Codync + <offset>` frames without addresses; read them by hand
(below). From 2.19.2 on, iPhone crashes start new issues, since their frames changed shape.

## Reading a crash

- Error tracking, filtered to `process = ios`, `env = main` (versions above the latest `v*` tag
  are a fork's). Frames run oldest first, the crashing one last.
- Codync's own frames (`Codync`, `CodyncWidgets`, `CodyncNotificationService`) show function,
  file and line. System frames (`SwiftUI`, `UIKitCore`, `libswift_Concurrency`, …) stay
  `binary + offset`: Apple publishes no symbols for them.
- The exception type and signal narrow it down: `EXC_BREAKPOINT` / `SIGTRAP` is a Swift trap
  (force unwrap, `precondition`, a continuation resumed twice), `EXC_CRASH` / `SIGABRT` an abort
  or an Objective-C exception (AVFAudio rejecting a microphone format), `EXC_BAD_ACCESS` /
  `SIGSEGV` a bad memory access.
- What led up to it: the same session's events (`app_opened`, `computer_paired`,
  `voice_call_started`, …), on the issue's page or in SQL:

  ```sql
  SELECT event, timestamp FROM events
  WHERE timestamp >= now() - INTERVAL 7 DAY AND properties.$session_id = '<session id>'
  ORDER BY timestamp
  ```

## Versions before 2.19.2, by hand

1. Find the version's Xcode Cloud run with the App Store Connect API (key and ids as in
   `tools/asc-submit.py`): `GET /v1/ciWorkflows/44342080-ce08-4dc1-be35-34ce6cbb41b5/buildRuns?sort=-number`,
   the run whose `sourceCommit.commitSha` is the tag's commit → `/v1/ciBuildRuns/<id>/actions` →
   `/v1/ciBuildActions/<id>/artifacts` → the `ARCHIVE` artifact → `/v1/ciArtifacts/<id>` gives its
   `downloadUrl` (a zipped `.xcarchive`, about 30 MB, with `dSYMs/Codync.app.dSYM`).
2. Old frames give the offset in decimal. Add it to the default load address and look it up:

   ```sh
   atos -o Codync.app.dSYM/Contents/Resources/DWARF/Codync -arch arm64 -l 0x100000000 \
     "$(printf '0x%x' $((0x100000000 + 5949552)))"
   # URLSessionSocket.ping() (in Codync) (HostConnector.swift:99)
   ```

   `dwarfdump --uuid` on the dSYM shows which build it belongs to.

## Uploading dSYMs by hand

- **Actions → iOS dSYMs → Run workflow** with the version: after a `Submit iOS` run started by hand
  (the automatic run only follows tag runs), when a run failed, or to upload an older version.
  Uploading the same dSYMs again does nothing (PostHog keeps them by UUID).
- **Locally** (macOS): `npm install -g @posthog/cli@0.18.9`, then with `ASC_KEY_ID`,
  `ASC_ISSUER_ID`, `ASC_PRIVATE_KEY`, `POSTHOG_CLI_API_KEY` and `POSTHOG_CLI_PROJECT_ID=649427`
  set, `python3 tools/upload-dsyms.py v2.19.2` from a checkout with the tags (PyJWT installed).
  `POSTHOG_CLI_DRY_RUN=true` checks the download and lists the dSYMs without uploading or a key.
- What's uploaded: PostHog's error tracking settings (symbol sets), or the PostHog MCP
  `error-tracking-symbol-sets-list`.

## Checking the whole path

Done once for 2.19.1 on 2026-10-19; repeat it after changing `CrashReports` or the workflow.

1. Upload a version's dSYMs (above).
2. Pick a function in that build: `nm <dSYM>/Contents/Resources/DWARF/Codync | xcrun swift-demangle`;
   its address minus `0x100000000` is the offset.
3. Get a real payload from `CrashReports.properties(callStack:…)` (a throwaway Kit test that
   builds MetricKit JSON with that build's `binaryUUID`, an `address` = any load address +
   offset, and writes the encoded properties to a file), then send it to `/batch/` as one
   `$exception` with `env = dev`, `$process_person_profile: false` and a test `distinct_id`.
4. A minute later, `properties.$exception_functions` and `$exception_sources` on that event name
   the function and file. Resolve the test issue in PostHog with a note that it was a test.

## The upload key

`POSTHOG_CLI_API_KEY` (GitHub secret, set 2026-10-19) is the personal API key "Codync iOS dSYM
upload (GitHub Actions)": only `error_tracking:write`, only the Codync App project. To replace
it, create a key with the same scope (PostHog → Settings → Personal API keys), run
`gh secret set POSTHOG_CLI_API_KEY --repo leepokai/Codync` with it, and delete the old key.

## When frames stay unreadable

| Symptom | Cause | Fix |
|---|---|---|
| Codync frames read `Codync + <offset>` on 2.19.2 or later | That version's dSYMs weren't uploaded | Check its `iOS dSYMs` run; run it by hand with the version |
| `iOS dSYMs` was skipped | `Submit iOS` ran by hand, not on a tag | Run `iOS dSYMs` by hand |
| "no Xcode Cloud archive … among the latest 30" | The release didn't change the iPhone app, or it's older than the last 30 runs | Nothing to upload, or upload it locally |
| "Xcode Cloud build N failed" | That archive failed; the version wasn't built | Upload the dSYMs of the run that shipped |
| `Failed to run dwarfdump` | Not on macOS | Run on macOS (the workflow uses `macos-latest`) |
| posthog-cli 401 or 403 | Key revoked, or missing `error_tracking:write` on Codync App | Replace the key (above) |
| Only system frames, no Codync frame | The crash happened inside the system (a SwiftUI update) | Look at the frames just before the system ones and at the session |
