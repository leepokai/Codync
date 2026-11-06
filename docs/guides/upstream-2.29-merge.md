# Upstream 2.29 integration

## Scope and state

Worktree: `C:\Users\Edwin Villeda\Documents\Codync-2.29`; branch: `integrate/upstream-2.29`.
Baseline: `af33eda40df9eeef45720eb7fe8c8f3530608bf3`, which contains `7f4145c8` plus the subsequent fork testing guide.
Target: `a30c8f611b1dd0265924f168eb7e808c68553f87` (upstream v2.29.0).
No rebase, squash, force push, new tags, version bump, PR, branch/worktree deletion, production app or signing-key modification is authorized or performed.
Intermediate manifests follow their upstream merge source; final manifests must follow v2.29.0, not a proposed future release.

**v2.27.0 and v2.28.0 are merged; v2.29.0 is merged with every gate green (the four process-tree regressions were fixed afterwards).** Validation records observed commands, never an assumed pass.

## Strategy

The initial uncommitted v2.27 merge had 111 conflicted paths. It was aborted within this worktree as explicitly authorized.
Strategy B integrates first-parent upstream PR merges chronologically: `14f57c52`, `9876308b`, `1dfad2eb`, `34af7b45`, `21de5613`.
The first PR already contains substantial upstream changes since the actual shared ancestor (`e770dbd9`, upstream 2.24).
Each step resolves host, Android core, Android app/design, desktop, iOS, documentation/CI; resolved paths are staged per layer, then one merge commit closes the step.
A `config.worktree` file enables rerere and merge commands explicitly use `-c rerere.enabled=true`, avoiding shared-config changes.
Worktrees named in the request are used only for reads; all edits and builds occur in the new worktree.

## Preservation checks

Run `node docs/guides/upstream-2.29-checklist.mjs` after each release stage.
It checks miniapp transport and runtime registration, proactive scheduling/exports, pending decisions, Android catalog/creation/pairing, desktop controls/tests, all original research documents by Git blob identity, and baseline ancestry.
The baseline checklist passed after creation: 18 original research documents unchanged.
Structural assertions and ancestry do not alone prove behavior; build, regression and functional checks remain required.

## v2.27.0, substep PR 216 (`14f57c52`)

### Conflict decisions

- `.github/workflows/kit.yml`: Keep fork build/dd result bundle, summary and failed-diagnostic artifact. Upstream provides equivalent reporting at a different temporary path.
- `apps/desktop/package-lock.json`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/package.json`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/main/index.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/main/ssh.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/client/host-client.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/index.html`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/store/bot-store.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/BotRoster.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/ChatWindow.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/call/CallView.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/call/GeminiLiveEngine.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/call/OpenAIRealtimeEngine.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/settings/SettingsView.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/thread/ChatRows.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/thread/Composer.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/thread/DetailsPanel.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/thread/ThreadView.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/renderer/views/thread/TraceView.tsx`: Combine upstream and fork behavior; see desktop resolution details below.
- `apps/desktop/src/shared/models.ts`: Combine upstream and fork behavior; see desktop resolution details below.
- `docs/architecture/conversation-runtime.md`: Keep watchdog and synchronous collaboration documentation; add upstream lead and deferred-message semantics.
- `docs/architecture/file-structure.md`: Union feature ownership references, avoiding duplicate team-management row.
- `docs/features/chat-messages.md`: Retain scratchpad/history and both collaboration APIs.
- `docs/features/marketplace.md`: Keep catalog/teams and add upstream shared Hermes memory explanation; duplicate Hermes installation text appears once.
- `docs/plans/android-app.md`: Keep implemented fork memory/multi-computer/notifications/file-export behavior; add upstream New session/Library/details rather than restoring obsolete gaps.
- `host/Cargo.lock`: Start from upstream and regenerate using cargo generate-lockfile --offline (459 packages).
- `host/Cargo.toml`: Use upstream manifest plus fork rusqlite functions and windows-sys features; no independent version bump.
- `host/src/agent/auth.rs`: Use upstream lazy launch candidates and preserve account-specific environment overrides and model probes.
- `host/src/agent/backends.rs`: Use upstream extracted search_path/tests; move fork Grok and WinGet detection and tests into search_path.rs. Hermes detection already exists upstream.
- `host/src/agent/bot/mod.rs`: Keep fork skill/spend/stall modules plus upstream tool_entry and call attribution.
- `host/src/agent/bot/queue.rs`: Keep upstream enqueue helper and deferred replies; preserve Slack deduplication and targeted synchronous-ask cancellation.
- `host/src/agent/bot/session.rs`: Apply account environment overrides; use upstream per-candidate own-memory setup rather than applying twice.
- `host/src/agent/bot/turn.rs`: Keep fork frozen initial snapshot/side-session handling and upstream artifact reminders/call tracking. Distinguish synchronous requests from deferred requests.
- `host/src/agent/bot/updates.rs`: Combine loop/stall observation with upstream extracted tool bookkeeping and activity calculation.
- `host/src/api/bots.rs`: Keep all fork methods, async team wrapper, appRequest, messageResult, voice receipts and validated document lane. Add upstream call/artifact endpoints. Route validated lanes through messages::post_in_lane.
- `host/src/api/events/tests.rs`: Retain fork catch-up watermark regression and upstream artifact revision/concurrency tests.
- `host/src/background.rs`: Start Slack and proactive services alongside upstream lead/call services.
- `host/src/chat/context.rs`: Retain fork opening-marker tests and upstream bot-request instruction tests.
- `host/src/chat/group.rs`: Use upstream cited-member follow-up rounds; retain fork first-round routing/Chief selection.
- `host/src/chat/manage.rs`: Keep fork creator ownership limits and upstream own-routine instructions.
- `host/src/chat/mod.rs`: Register all fork modules plus upstream artifacts, calls, lead and library.
- `host/src/chat/team.rs`: Use upstream deferred message_bot routing, retaining synchronous ask_bot, batches, pipelines, wait-cycle graph, cancellation, envelope escaping and assignment ledger in sibling modules.
- `host/src/hub.rs`: Use upstream extracted validation and lead deletion protection; reapply account/approval/external validation, terminal cleanup and secrets cleanup. Cancel both synchronous work and active routing on deletion.
- `host/src/mcp.rs`: Retain fork concurrent request cancellation; upstream wording change adds no behavior.
- `host/src/routines/mcp.rs`: Combine fork watch-trigger guidance and upstream own-routine/delegated-test guidance.
- `host/src/store/bots.rs`: Exclude both fork side conversations and upstream live-call replies from unread counts.
- `host/src/store/entries.rs`: Keep fork side-conversation query plus upstream Library queries. Add exact call-notice lookup for receipt enrichment.
- `host/src/store/mod.rs`: Keep connection helper and ProactiveSettings exports; add upstream revision-stamped kv transaction.
- `host/src/tui/app/model.rs`: Use upstream extracted bot model; reapply optional account to app/bot.rs.
- `host/src/tui/app/navigate.rs`: Retain fork filter/reorder guard and use upstream lead-aware roster bands.
- `host/src/tui/app/overlay_keys.rs`: Keep Documents/Computer actions and add Library.
- `host/src/tui/app/ui.rs`: Union fork and upstream imports, actions and menu entries.
- `host/src/tui/manage/fields.rs`: Keep fork voice decisions/protected-fill submits and upstream artifact rename.
- `host/src/tui/manage/mod.rs`: Union document/protected-fill/voice and Library exports/submits without duplicate handoff export.
- `host/src/tui/sheets.rs`: Retain computer/documents/tools sheets and add Library sheet.
- `host/src/tui/view/roster.rs`: Keep project grouping, collapsed sections and stopping feedback; add upstream lead star/band handling.
- `host/src/tui/view/transcript.rs`: Keep fork voice-receipt actions and upstream unfold semantics.
- `host/src/usage.rs`: Keep fork refresh lock and upstream near-limit budget guard.
- `host/tests/fixtures/team_agent.py`: Assert required tools as a subset so upstream and fork collaboration tools coexist.

### Desktop resolution details

- Main registration keeps fork SSH screen targets and adds upstream SSH configuration; moved SSH validation receives upstream isSSHHost while retaining native Windows key paths.
- HostClient/store sends carry both document threadId and call; catalog, miniapp, proactive, token-budget, diagnostics and portable-bundle methods remain.
- Renderer CSP retains loopback WebSocket permission and adds upstream codync-artifact frames.
- Roster keeps filter safeguards, previews and revealed actions, with lead drag protection and upstream computer-section dragging.
- Call engines retain fork receipt capture and upstream host-owned instructions/tools/transcription. End the host call before recording optional receipt metadata with the same callId.
- Receipt logging enriches the existing host call notice atomically rather than adding another row; title writing also uses atomic entry edits. A regression checks stable row ID, title and idempotency.
- Thread main transcript filters side conversations and call messages together. App/artifact/call callbacks coexist; receipt confirmation remains alongside call opening.
- Composer has separate drafts for document lanes and artifacts; external-terminal safeguards remain.
- Details has Apps, Library, Memory, Documents, Computer, Settings and existing group documents.
- Upstream extracted WorkingIndicator replaces the fork's last-thought-only widget: its richer toggle exposes narration, thinking, tools and plans. Document replies use it too. Trace filters remain with upstream follow-end behavior.
- Settings retains diagnostics/research and adds upstream hidden-bot controls.
- package.json/lock begin with upstream, reapply fork dependencies and VNC test plus upstream call tests. npm regenerates the lockfile.

### Validation observed so far

- Baseline checklist: exit 0; 18 research documents unchanged.
- `cargo generate-lockfile --offline`: exit 0; 459 packages.
- `cargo fmt`: initial parse check succeeded after team extraction; final check pending later edits.
- `cargo clippy --all-targets -- -D warnings`: running with CARGO_INCREMENTAL=0 and CARGO_BUILD_JOBS=1; not yet passed.
- `npm install --package-lock-only --ignore-scripts`, `npm ci`: exit 0; npm reported 4 dependency advisories (1 moderate, 3 high); no unrelated dependency upgrade applied.
- `npm run typecheck`: initial diagnostics exposed moved WorkingIndicator/Reveal imports and unused imports; fixes applied; final command exited 0.
- `npm test`: exit 0; 346 passed, 19 platform skips, 0 failures (365 tests).
- Android build/tests/lint: not started while Rust compilation is active.
- iOS: not compiled on Windows; syntax/coherence review and git diff --check performed; Xcode/xcodegen unavailable.

## Android resolution review

- Core BotDraft/Conversation: retain proactive/account fields and document lanes, upstream sorted readable templates and call metadata. Templates omit fork device-account/team-management fields.
- Core OpenAI/Gemini/Voice: upstream host CallLink owns session credentials/tools/live transcript; fork optional bounded recorder survives under CallReceiptTranscript (renamed from conflicting CallTranscript), including generated/queued/recognized source and interruption limits. Existing recorder tests adapted.
- AppStoreVoice: end host call before logging optional receipt; host enrichment returns one call row. Chat preserves both opening the upstream call conversation and fork confirmation controls. On-device/cloud fallback and optional receipt preference survive.
- WidgetFeed/LeadBotTile: upstream mostUrgentBot name with fork needs-input-first/latest ordering; Quick Settings active tile refresh retained. Old leadBot references adapted.
- AppStoreConnection: receive upstream artifact events and preserve fork scoped reading acknowledgements.
- AppStoreDrafts/DraftBook/MessageComposer: fork document/chat lanes, camera/photo annotation, terminal read-only composer, haptics, quote/bookmarks/clipboard preserved. Upstream artifact drafts get an explicit persisted draft key without changing recipient bot or creating a thread. New regression test covers separate persisted lanes.
- AppStoreFiles: retain deferred share sheet in its original conversation; add upstream Library download overload.
- BotEditor/BotSettingsForm/BotDetailsView: start from upstream extracted editor/form/details. Reapply account/model picker, robot avatar preview, coordinator, proactive, connector failures, saved documents/computer activity. Apps and Memory remain available as details tabs, beside upstream Library and Settings; fork miniapp decisions remain intact.
- BotListView/BotRow/BotRosterActions: fork search/filter/project sections, split-view highlight, swipe/peek/accessibility actions and stop status retained in extracted row. Upstream lead ordering/star/immutable roster restrictions and confirmed New session added to shared actions.
- CodyncApp: retain fork tabs, split navigation, shared intake, pairing and simplified NewBotView/NewGroupView. Chat title opens upstream richer details while roster keeps full settings editor.
- ChatScreen/ThreadHeader/RepliesView/ChatRows/BotNoticeCards/WorkingIndicator: upstream artifacts/call viewing and step disclosure integrated with fork responsive header/search, accent, document conversations, swipe-to-reply, new-message counter, camera and shared-workspace warning. Extracted siblings keep changed files small. Thinking disclosure respects upstream Settings; fork shimmer retained via shared primitives.
- RoutinesView: adopt upstream embedded routine rows and ready-to-edit guards; keep existing standalone RoutinesScreen entry point as a wrapper for fork document/inspector callers. Fork routine editor details unchanged.
- MainActivity/SettingsView/Controls/TraceView: preserve biometrics receiver, rotation/navigation/appearance/security controls and typed trace filters; add upstream thinking-settings initialization, card heading styles and expanded running-step trace.
- Tests: preserve/restore upstream WorkingIndicatorTest and TraceTest (fork had deleted earlier APIs), adapt scrolling test to fork ConversationScroll; adapt LiveMemory/LiveRoutines navigation to extracted settings/details. Instrumented tests were not run by this editor.

Validation: no Gradle or device commands run (Rust build owns RAM); no conflict markers remain in Android sources. Files remain unstaged for coordinator review and build. No miniapp hardening or proactive filtering fixes included here; those are separate requested commits.

## iOS resolution review

All changes are restricted to `Codync-2.29`. No staging, commits, builds or app launches were performed by the iOS editor.

- `apps/ios/Kit/Sources/CodyncKit/Models/Models.swift`: accepted upstream's extraction of `Bot` and its decoder into `Models/Bot.swift`; retained the fork's other additive model fields (`Entry.threadId`, `UsageProvider.status`, `BotDraft.account`). This avoids duplicate `Bot` declarations.
- `apps/ios/Kit/Sources/CodyncKit/Models/Bot.swift`: reapplied the fork's optional agent `account` field and lenient decoding at the new location. Upstream's `lead`, roster band and shared harness-memory fields remain.
- `apps/ios/Kit/Sources/CodyncUI/Bots/BotDetailsView.swift`: combined upstream's conditional Library tab with the fork's Documents tab. Both destinations, Memory and Settings remain available.
- `apps/ios/Views/BotListView.swift`: combined fork search/filter/project/history state, context preview, Open chat and roster Stop action with upstream New session confirmation and lead restrictions. The lead cannot be moved, pinned, hidden or deleted. Fork filtered-reorder safeguards remain.
- `apps/ios/Kit/Tests/CodyncUITests/StoreTests.swift`: retained the fork's test extraction to `StoreTestFixtures.swift` and `StoreConnectionTests.swift`. All upstream connection-test names already exist there; they were not duplicated. Removed only the obsolete `callOperatorToolsDriveTheBot` test, because upstream moved voice tools into the host and removed the client `CallOperator` type.
- `apps/ios/Kit/Tests/CodyncUITests/StoreTestFixtures.swift`: reapplied upstream's scripted enqueue-failure support to the extracted fixture, preserving the fork's checkpoints and existing actor isolation.
- `apps/ios/WhatsNew.md`: retained fork 2.26.0 notes and combined both sets of 2.25.0 notes under one section with zh-Hant and en-US subsections. No new release version was introduced.
- `apps/project.yml`: accepted the PR's upstream settings/version (2.25.0 at this intermediate step). The fork had only a version difference, not additional dependencies. Did not edit `project.pbxproj`.

## Voice coherence

The fork baseline `af33eda4` has no iOS `CallTranscript`, `VoiceReceipt`, `voiceTranscript` or `confirmVoiceDecisions` implementation (`git grep` found none). Its call view logged only duration through `logCall`. Upstream's `CallLink` now serializes live transcript delivery, tool calls and `endCall`, and `BotStore+Calls.swift` upserts the single host-owned call notice. This upstream behavior replaces the old iOS duration-only path; an extra legacy `logCall` with no optional receipt data is unnecessary. No recorder type collision exists in iOS (unlike Android). Legacy receipt enrichment for Android/desktop is handled by the coordinator in the host.

## Validation

- No remaining conflict markers under `apps/ios` or `apps/project.yml` after resolution (`rg` found none).
- Static declaration check found one `Bot`, one `FakeRemote`, one `until`, and one visible-conversation acknowledgement test after moving/reusing the extracted code.
- Verified `StoreTests.swift` differs from its fork stage solely by removal of the 38-line obsolete client-operator test.
- Reviewed tab enum/switch completeness, context-menu return construction, optional account decoding and fixture actor isolation.
- iOS: **not compiled**. `xcodebuild`, `xcodegen` and Swift are unavailable on this Windows machine. Xcode project regeneration and actual Swift 6/iOS Simulator checks must run on macOS. The auto-merged upstream generated project was not manually edited.

## v2.27.0, substep PR 219 (`9876308b`)

### Conflict decisions

- `host/src/api/mod.rs`: dispatch keeps the fork's `catalog` lane and adopts upstream's caller argument for `bots::call`.
- `host/src/api/bots.rs`: `logCall` keeps the fork's receipt-enriching handler; `startCall` adopts the device owner upstream needs to route its voice cues.
- `host/src/analytics/reported.rs`: the reported-event table keeps the fork's onboarding events and adds upstream's `voice_call_quality`.
- `apps/android/.../AppStore.kt`: a call still ends when its bot is deleted, and the fork's visible-entry filter feeds the state; upstream's event cues now drive speech, so the client reply gate goes.
- `apps/android/.../AppStoreConnection.kt`: keeps the fork's per-scope read acknowledgements and adds upstream's `hearCalls(batch)`.
- `apps/android/.../AppStoreVoice.kt`: the binding keeps the fork's optional receipt transcript; its constructor drops the reply gate (the host decides what a call hears).
- `apps/android/core/.../Voice.kt`: adopts upstream's rewritten audio phases and cue-driven speech; the fork's reply gate is removed because the host announces only public results.
- `apps/android/.../VoiceControls.kt`: the connecting state reads `phase.connecting` with the fork's `callStatusText`.
- `apps/desktop/.../CallView.tsx`: unmount keeps the fork's receipt log after the host session ends, then reports quality; the unused `watchVoiceCall` import is gone.
- `apps/ios/.../BotStore+Sync.swift`: adopts upstream's `.call(cue)` case; entry-based speech is the host's job now.
- `apps/ios/Kit/Tests/CodyncUITests/StoreTests.swift`: starts from upstream and re-applies the fork's extraction (fixtures and seven connection tests live in `StoreTestFixtures.swift` and `StoreConnectionTests.swift`); upstream's `startCall` fixture, `hear:` API and new cue test stay.
- `apps/ios/WhatsNew.md`: one 2.26.0 section with both sides' notes.
- `docs/architecture/file-structure.md`: the voice row carries upstream's `announce`/`speech`, the Live Activity and the Android call notification, keeping the fork's profile, management, approval and watchdog rows.
- `apps/android/core/src/test/.../EntryVisibilityTest.kt`: the two client reply-gate tests are replaced by the mirror contract they now rely on.

### Validation observed

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --bin codync-host -j 1 -- --test-threads=1` exit 0 (711 passed, 1 ignored).
- `npm run typecheck` exit 0; `npm test` exit 0 (362 passed, 19 platform skips, 0 failures).
- Android `assembleDebug :core:test :app:testDebugUnitTest lintDebug` exit 0 (104 tasks).
- iOS: not compiled here; only static review.

## Requested hardening fixes (committed before the sub-merge)

- Host proactive visibility uses a shared internal-entry flag in storage, actors and every client. An exact `NO_UPDATE` stays private during streaming or an explicit send; a useful final is published without its scheduled prompt. Scheduled turns do not coalesce with a person's queued messages or become recovered user requests. Host queries, mirrors, trace, speech, approvals, quotes, bookmarks and Library honour visibility. Regression tests cover real ACP fixtures, store queries, stale mirrors and client classification.
- A scheduled turn's replacement of a published artifact no longer overwrites or hides it: the new HTML waits as `<id>.staged.html` beside its own metadata and lands only when the turn promotes a useful result. A silent, failed or stopped check-in leaves the user's version and title untouched, and routines, rooms and bot requests still publish straight into the Library, as before.
- Android miniapp navigation authenticates the exact virtual HTTPS origin, the current capability and native main-frame metadata. External HTTP(S) opens through the system browser; unsafe navigation is blocked. Unsupported WebViews show a clear update instruction instead of installing an unsafe fallback. The other clients do not expose this Android JavaScript interface; this hardening is platform-specific.
- Android bridge bodies reject FormData, Blob, streams, typed arrays and Request bodies explicitly rather than silently submitting empty content, and a GET/HEAD body is refused. The secure listener uses [Android's WebMessageListener API](https://developer.android.com/reference/androidx/webkit/WebViewCompat.WebMessageListener), with the [stable WebKit dependency](https://developer.android.com/jetpack/androidx/releases/webkit) selected against the app's SDK. Nine JavaScript-template tests pass, including socket polling and foreign Request handling.
- Scheduler redundant borrows were removed, and storage deletion/list errors are logged with tracing. A new appRequest regression publishes a page and checks page/data, bad capability (403), and invalid paths (400).
- Desktop post-filter typecheck and tests exited 0: 348 passes, 19 platform skips, 0 failures.
- Initial Android gate exited 1 at compileDebugKotlin: ScreenHeader's third argument had become Boolean, AppearancePreferences.initialize was obsolete, and canEdit lacked its extension import. Fixed as a named trailing slot, the current preference accessor and the import. The full gate then exited 0: `assembleDebug`, `:core:test`, `testDebugUnitTest` and `lintDebug` (109 tasks); a forced rerun of both test tasks on this tree executed 46 tasks and exited 0.
- Host gates on the committed tree: `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test -j 1 -- --test-threads=1` exit 0 over 22 targets (699 unit tests, plus the workspace, call, routine, team and turn end-to-end suites). The proactive ACP fixture now reads and writes UTF-8 explicitly, because the Windows console codepage turned its prompts into lone surrogates.

## v2.27.0, substep PR 221 (`34af7b45`)

This PR brings the Extensions redesign, the host update request, private-relay labels and the WorkBuddy agent.

### Conflict decisions

- `host/src/agent/acp.rs`: keeps the fork's `Answer` drop (a response is consumed once) and takes upstream's `rpc_error`, which the merged reader now calls.
- `host/src/agent/auth.rs` (+ `auth/agent.rs`, `auth/models.rs`): upstream extracted the launch and the model discovery; the fork's account environment is re-applied inside them (`start_with_env`, exported for account sign-in and the Antigravity probe; `models(store, backend, account)`).
- `host/src/agent/bot/session.rs`: starts with upstream's `launch_env` (the harness's own variables) plus the fork's account overrides.
- `host/src/main.rs`: keeps `cli` and `sentinel` and adds upstream's `cloud_cli` and `secret_prompt`.
- `host/src/market/skills.rs`: adopts upstream's public tree page and single-file reader (the contents API's 60-request quota). Both stay inside the fork's `GITHUB` egress policy, and the size limit is read through `read_limited`, so the loopback fixture still tests it.
- `host/src/tui/manage/{market,mod}.rs`, `sheets.rs`, `sheets/market_view.rs`: upstream's rail/panel window with the fork's Bots and Teams tabs (`Tab::index` and `Market.pages` cover five tabs, catalog rows, `market_page` in the tools shortcut).
- Android `MarketplaceView.kt`: upstream's paged tabs, error line and credentials header, with the fork's modal inset, reselect-to-top, "No computer paired" state, ready-made catalog block and the Composio Apps list. `MarketSection` is restored to `MarketRows.kt` for those blocks, and `ComposioViews.kt` pages with `CatalogPaging` now that upstream's `CatalogPage` holds only `items`.
- Android `BotListView.kt`, `ChatScreen.kt`, `RepliesView.kt`: the fork's roster and composer stay, and all three pass `store::requestHostUpdate` into upstream's update card. `ConnectionPresentation.kt` keeps the fork's status comment and adds upstream's private-relay labels; `SettingsView.kt` keeps the fork's computer row and uses `routeTitle(route, computer.cloud)`; `UpdateNeededCard.kt` takes upstream's signature and the fork's host-name fallback.
- Desktop `MarketplaceView.tsx`: upstream's rail/panel window with the fork's ready-made catalog as two more sections (`market-models.ts`, new `CatalogPanel.tsx`; `CatalogSections.tsx` removed, and the checklist records the move). The fork's `AgentCard` gives way to upstream's in `MarketplacePanel.tsx`.
- Desktop `package.json` (test glob), `main/tray.ts` (the fork's "Pair phone." with upstream's QR icon) and `store/bot-store.ts` (both `SentinelReport` and `SharedFile`): union.
- iOS `WhatsNew.md`: upstream's 2.26.2 in both languages, then 2.26.1, then the fork's older sections.
- `docs/features/marketplace.md`: the fork's teach flow and ready-made catalog plus upstream's tree-page and WorkBuddy text.
- `.github/workflows/kit.yml`: the fork's `build/dd` result bundle and summary plus upstream's retry-on-failure. `release-desktop.yml`: the fork's `MAC_SIGNING` gate plus upstream's `needs.check.outputs.package`.

### Validation observed

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --bin codync-host -j 1 -- --test-threads=1` exit 0 (738 passed, 1 ignored).
- `npm run typecheck` exit 0; `npm test` exit 0 (374 passed, 19 platform skips, 0 failures).
- Android `assembleDebug :core:test :app:testDebugUnitTest lintDebug` exit 0.
- Preservation checklist exit 0, including the recorded `CatalogSections`/`CatalogPanel` move.
- iOS: static review only.

### Fixed while merging

- The watch fingerprint no longer hashes a skipped folder's own mtime (`.git` changes on every write), which made `a_folder_fingerprint_changes_with_its_files` fail or pass by timing.
- `Tab::index` and `Market.pages` follow the five Extensions tabs.

## v2.27.0, substep PR 226 (`21de5613`)

This PR finishes v2.27.0: usage notes on limits, the Extensions lists as lazy lists, and the add/open rules for connectors and agents.

### Conflict decisions

- `host/src/usage.rs` was split by upstream into `host/src/usage/` modules. The fork's account-aware `Provider` (`String` id/name plus `status`), its refresh lock and its Antigravity per-account probe are re-applied in `usage/mod.rs`; `Window` keeps upstream's new `text` notes.
- `host/src/usage/antigravity.rs`: keeps both sources — the fork's adapter probe (`refresh`, per account, with `status`) and upstream's language-server reader (`collect`), whose `parse` is now `parse_quota`; the language server only fills a base reading the adapter didn't report.
- Terminal Extensions (`tui/manage/{market,mod}.rs`, `tui/sheets/market_view.rs`): upstream dropped paging; the fork's Bots and Teams tabs stay as flat lists (`market_rows`, five `TABS`, `botCatalog` still requested, catalog and Apps reachable with the same keys).
- `tui/view/usage.rs`: upstream's scrolling sheet with the fork's provider `status` line (`Line::Status`), so an agent that can't report says so.
- Android `Usage.kt`: the fork's `status`/`statusLabel` plus upstream's `limits` (windows without a note) and `tightest`.
- Android `MarketplaceView.kt`: upstream's lazy lists and tabs, with the fork's modal inset, reselect-to-top, "No computer paired" state, ready-made catalog above every section and the Composio Apps list under Connectors; the paging helpers upstream removed are gone.
- Desktop `IslandUsage.tsx`, `UsageLimits.tsx`, `market-models.ts`, `MarketplaceView.tsx`: upstream's usage-note rendering (`windowReading`) with the fork's provider status lines, catalog sections and tooltips; the pager helpers upstream deleted are gone.
- `docs/architecture/desktop-app.md`: Windows gaps keep the fork's implemented SSH tunnels and add upstream's "each agent's own report". `docs/features/marketplace.md`: the fork's teach flow plus upstream's rules for opening a skill, a connector and an agent sheet.

### Validation observed

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --bin codync-host -j 1 -- --test-threads=1` exit 0 (765 passed, 1 ignored).
- Desktop: `npm run typecheck` exit 0; `npm test` exit 0 (372 passed, 19 platform skips, 0 failures).
- Android `assembleDebug :core:test :app:testDebugUnitTest lintDebug` exit 0 (104 tasks); the Composio Apps list follows upstream's paging removal (one list, no "Load more").
- iOS: static review only.

## v2.27.0 gate

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0.
- `cargo test -j 1 -- --test-threads=1`: the unit suite (765 passed, 1 ignored) and every end-to-end target pass. Under the full-suite run on this loaded machine `routine_retry_e2e::a_run_that_hit_its_time_limit_waits_for_the_user` failed with "routine session setup timed out" (its 2 s deadline); the target alone passes 5/5 and the single test 3/3. Recorded as an environment flake, not as a code failure.
- Android `assembleDebug :core:test :app:testDebugUnitTest lintDebug` exit 0 (104 tasks).
- Desktop `npm run typecheck` exit 0; `npm test` exit 0 (372 passed, 19 platform skips, 0 failures).
- Preservation checklist exit 0.
- iOS: not compiled here; Swift changes are reviewed, not built.

## v2.28.0 merge (PR 228)

A single merge for the release: codync-network, cross-computer bot messages, cloud-assistant tasks and lead badges.

### Conflict decisions

- `host/Cargo.toml`: the fork's `rusqlite` `functions` feature plus upstream's `rustls`; the workspace gains `crates/network`.
- `host/src/main.rs`, `host/src/api/mod.rs`: the fork's `proactive`, `origin`, `rebinding` and `catalog` lane stay and upstream's `network`/`network_cli` modules and `network::call` lane are added.
- `host/src/api/bots.rs`: upstream's async `teamCall` with the fork's `messageResult`.
- `host/src/chat/team.rs` (9 blocks) and `tests.rs`: both flows survive — the fork's `ask_bot`/`delegate_batch`/`run_pipeline`, wait graph, reply oneshot and reserved sender slot, and upstream's `Back::Local/Remote/Assistant`, `remote::answer`/`cancelled`, `assistant.started/answered`, `delegationId`-tagged notices and the network route. Blocking asks build a local back; `Drop` releases a slot only when the message reserved one and stayed local; envelope escaping and the consent rule live in the shared prompt builder.
- `host/src/chat/team/blocking.rs`: builds `Back::Local { sender }` now that `MessageLifetime` keeps the back instead of a sender field.
- `host/src/agent/auth/models.rs`, `agent/registry.rs`, `mcp.rs`, `usage/mod.rs`: the fork's account environment, egress policy, tool timeouts, usage refresh lock and status lines, with upstream's `Duration::from_mins/from_hours`.
- `host/tests/common/mod.rs`: the fork's authenticated readiness plus upstream's start diagnostic (`not_started`) and unowned-host cleanup.
- Android `BotListView.kt` drops upstream's duplicate `BotRowItem` (the fork's `RosterBotListEntry` in `BotRow.kt` is the row); `ChatRows.kt` keeps the fork's `Type.chatSubheadline` with `state.authorName`.
- `CLAUDE.md` and `docs/architecture/file-structure.md`: the fork's pointers and rows plus upstream's cloud connector, `codync-network` and local-secrets notes, with one row per responsibility.

### Validation observed

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0 after the fork's six duration constants moved to `from_mins`/`from_hours`; `cargo check --all-targets` exit 0.
- `cargo test --bin codync-host` **could not run on this machine**: rustc aborts while generating the test binary with `Allocation failed` / `STATUS_STACK_BUFFER_OVERRUN` (LLVM out of memory on 8 GB shared with other work). The type check and clippy cover the same code; the release's runtime gate needs a machine with more memory.
- Android and desktop gates were not re-run after this merge (their files are the label and author changes above); the v2.27.0 results stand for the code they covered.
- iOS: not compiled.

## v2.29.0 merge (PR 232)

The release merge: network rules, address names and menu entries, one agent-and-model picker on both clients, a per-computer push switch and settings as its own Android screen.

### Conflict decisions

- `host/src/agent/acp.rs`, `process.rs`, `agent/mod.rs`: upstream's newer process ownership (monitor, graceful `stop_within`, suspended start) with the fork's per-account environment isolation at spawn.
- `host/src/agent/bot/{launch,session,turn}.rs`, `chat/context.rs`: upstream's unprompted-snapshot flow, session-opening timeout and `drop_unanswered`, plus the fork's allocation guard, `finish_session_model`, resumable replacement, side-conversation opening and derived `unprompted` mirror.
- `host/src/chat/calls/context.rs`, `tui/app/{forms,ui}.rs`, `tests/fixtures/wire-shape.json`: the fork's JSON fixture, account test and network-opened lists; the action and field tables now hold 21 and 20 entries.
- Android: upstream's slim Settings page and `ComputersView` over the fork's four-destination shell (rail, split view, reselect, per-computer push), the fork's catalog/proactive/account fields inside upstream's combined picker, and the fork's multi-computer pairing on the merged store API.
- Desktop: upstream's compact picker and Settings page with the fork's account picker, robot preview, coordinator and proactive sections; one bot template now (`@shared/bot-template`, omitting account, team management and network reach).
- iPhone: upstream's picker and settings with the fork's account support; `WhatsNew.md` regains its eaten 2.28.0 section.
- Docs: the fork's studies and Android plan plus upstream's network plan and ownership row.

### Validation observed

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo check --all-targets` exit 0.
- `cargo test --bin codync-host`: 800 passed, 0 failed, 1 ignored. The four process-tree regressions that failed after the merge (`agent::acp::tests`) are fixed in `fix(host): release an agent's process tree when its leader exits`: the owned tree is dropped before the stdin pipe, the leader starts suspended so its children join the job, Windows walks the leader's tree with Toolhelp, and the reader stops when the leader is gone.
- Android `assembleDebug :app:assembleDebugAndroidTest :core:test :app:testDebugUnitTest lintDebug` exit 0.
- Desktop `npm run typecheck` exit 0; `npm test` exit 0 (385 passed, 19 platform skips, 0 failures).
- Preservation checklist exit 0.
- iOS: not compiled.

## Final verification (Windows, 29 Oct)

- `cargo fmt --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0.
- `cargo test -j 1 -- --test-threads=1`: the unit suite (800 passed, 1 ignored) and all 23 end-to-end targets pass. The `memory_e2e`, `routine_retry_e2e` and `routines_e2e` regressions the suite found are fixed (`44fc9384`, `4ea15384`): the first instructions stay owed until a prompt succeeds, a cut-short non-resumable agent takes the retry, and a hidden resume continues a reloaded session instead of refusing it.
- Functional end-to-end with an isolated host on port 19224 (`python .integration/e2e.py --port 19224`): all six checks pass — isolated Dev host, a bot created over the API, a miniapp published and served, a wrong capability answering 403, traversal and backslash paths answering 400, `_data` POST then GET, and a real proactive check-in that advances the `proactive:<bot>` ledger with no sentinel and no scheduled prompt in public history.
- Preservation checklist exit 0.

## Resource and environment evidence

- Initial C: free space: 25.14 GiB. Checked again before each heavyweight compiler.
- During the Android gate free space approached 21 GiB; NTFS compression was applied only to this worktree's regenerable host/target cache (569,971,671 logical bytes stored in 285,120,934), compact exit 0.
- During the v2.27.0 gate the volume reached 0 bytes free twice, and end-to-end hosts then failed with `database or disk is full` (`SQLITE_FULL`), not with assertions. Freed space by deleting only regenerable build output: this worktree's `host/target` (6.4 GB) and the `host/target` of the two abandoned attempt worktrees `Codync-integration` (last written 19 Oct) and `Codync-integrate` (24 Oct), 20.8 GB. No source, data, branch or worktree was deleted, and `Codync-windows-validation`, `Codync-2.29-recovery` and the Deska worktrees were left untouched.
- Host builds and tests ran with `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0` to keep the target inside the free space; this changes only debug info, not behaviour.
- Installed Codync Dev already owns port 19223: PID 22996, executable under LocalAppData/Programs/codync-desktop-dev/resources/codync-host.exe. It was left running. Functional harness will first record this conflict and may use explicit isolated port 19224; it accepts only a binary inside this worktree and fresh task-owned data.
- adb devices returned an empty device list. No signing key was read and no phone installation performed.

## Remaining release stages and functional verification

v2.27.0 completion, v2.28.0 and v2.29.0 gates/pushes remain pending. End-to-end new-host miniapp/scheduler validation is prepared but has not run. iOS/Xcode and real Android WebView/device checks remain unavailable here.

## Fork commit inventory

The following complete baseline inventory was captured against pinned upstream v2.29.0. Every commit must remain an ancestor of the integration; overlapping implementations are explained above and in subsequent release sections.

- `af33eda4` docs: testing guide for the mini-apps, proactive check-ins and pending decisions
- `7f4145c8` docs: keep the bdots and mercury-agent research that only lived in a worktree
- `b6bb8228` feat: remote mini-apps on phones, proactive check-ins and real pending decisions
- `d7c62751` fix(android): connect integrated phone workflows
- `80bf3762` fix(desktop): harden bot controls and mini-app sharing
- `a31e0d0b` docs: record completed Android validation
- `b23479ca` fix: stabilize integrated Windows app workflows
- `3e2b0134` Merge branch 'deska/Implementar-Bloque-E-Android-55c7c8ab' into integrate/windows-all-additions
- `b3902d34` Merge branch 'validation-snapshot/openstark' into integrate/windows-all-additions
- `cb6df040` Merge branch 'validation-snapshot/bdots' into integrate/windows-all-additions
- `8d7ed3f2` Merge branch 'deska/Verificar-Documento-de-Mejoras-Mercury-8691904c' into integrate/windows-all-additions
- `7e010817` chore: snapshot bdots for Windows validation
- `00d7f2c3` chore: snapshot openstark for Windows validation
- `79b045f9` feat: skill usage and dormancy (42), save-as-skill offer (41) and the doctor link
- `d73cbdae` feat(host): cap the skills a bot can save at 40
- `eccb8117` feat: run_pipeline (idea 10) and the Team run screen for pipelines and batches
- `81fe7410` docs: idea 10 is not implemented
- `c7b2a3a4` feat(host): delegate_batch, typed tasks with one digest for all bots
- `70b81e0a` feat(desktop,android): diagnostics screens for the bot check, token limits, failed runs and bot bundles
- `f988efad` feat(host): portable bot bundles and hung-tool notice
- `fd8e3799` feat(host): doctor, turn outcome, idle-routine pause, daily token budget, failed-run cleanup
- `ae919257` feat: stop all working bots at once (host stopAll, desktop roster button, Android computers menu)
- `6db7a3a5` docs: audit the mercury-agent ideas against Codync and fix source errors
- `f4640ef5` feat: simpler bot creation and a catalog of ready-made bots and coordinator teams
- `aef5891a` feat(host): report Antigravity plan usage and page providers in the island
- `b107d42c` feat: pair Android phones with Windows computers and add fallbacks from chat errors
- `30ae24ef` build: bump version to 2.26.0
- `eded5f5f` ci(host): release the archives without the signed update manifest when the key is missing
- `92d69ed2` Merge PR #3: upstream leepokai/Codync 2.24.0 into the fork (2.25.0)
- `2b0ee09f` Merge PR #2: integrate the Android and Windows improvements
- `c5332f91` feat(host): send --reply-to replaces --thread in the send CLI and headless script
- `bc6d3670` test: drop fleetManagement from the wire snapshot; finish the Kit store test port
- `7a7ec1f7` Merge branch 'integrate/android-windows-wip' into integrate/upstream-2.24
- `b2611356` test: adapt end-to-end and Kit tests to replies instead of message threads
- `68e8072f` test(host): run the sh quoting test on Unix only
- `62463857` Merge branch 'integrate/android-windows-wip' into integrate/upstream-2.24
- `e48f2b0f` fix: drop the removed pairing-window stylesheet import, the group editor's documents, obsolete thread allocation tests
- `5ed3a30d` test(host): record the additive wire fields
- `2dbf501c` Merge branch 'integrate/android-windows-wip' into integrate/upstream-2.24
- `ca3bf710` fix: adapt tests and clients to the merged design
- `6702983c` fix: computer activity e2e calls deleteBot with botId; skip the signed Mac build without signing secrets
- `686dc020` fix(android): keep the fork's routine editor rules and reading tests
- `8854478c` ci: rerun checks
- `e43dc919` Merge branch 'integrate/android-windows-wip' into integrate/upstream-2.24
- `c66c44a5` test(host): fix the terminal-session hook tests
- `4fbf06c5` Merge upstream/main (leepokai/Codync 2.24.0) into the fork
- `b53f1d31` fix(host): clear the Unix-only clippy lints
- `d89a95a9` fix: make CI green after the integration
- `6059064c` feat: integrate the Android and Windows improvements work in progress
- `47313706` fix: repair the build after the latest merges
- `754cb677` fix(host): compile the terminal-session hook module
- `d5e633da` fix(desktop): keep the demo bridge in step with dropped files
- `daa5adad` fix(host): compile the terminal-session hook module
- `a0427119` fix(desktop): keep the demo bridge in step with dropped files
- `edbcb3f8` Merge remote-tracking branch 'origin/main'
- `e99778ee` Merge branch 'feat/openmausbot-integration'
- `ca670f9b` feat(android): block D chat and content improvements
- `73981d45` Merge remote-tracking branch 'origin/main' into deska/Implementar-Protecci-n-SSRF-En-Rakazo-b5b6116f
- `2cb0f952` Merge remote-tracking branch 'origin/main'
- `c1d54ab2` feat: complete block E for Android and the desktop app
- `b7ae5e75` feat: integrate OpenMausBot runtime, sandbox, CDP browser, Chief of Staff, Sentinel, and memory budget
- `a95b2de8` feat: complete block E for Android and the desktop app
- `77349edb` feat: fleet management, approval rules, idempotent approvals, teach-to-skill
- `e7f55cfa` feat(desktop): working characters follow the pointer with their eyes
- `ee1625bd` feat(desktop): move the island between screens and drop files on it
- `2037c9d5` fix(host): harden watch triggers, instruction history, watchdog and scratchpad
- `5a50a475` test(host): a deleted terminal session stays deleted and frees its hook
- `82ae737c` docs: note the local HTTP transport of the terminal-session hook
- `06278688` feat(android): unlock risky approvals with biometrics
- `07bfb44b` feat(host): watch agent sessions running in your terminal
- `f6cb9953` feat(host): per-conversation scratchpad for a bot's working notes
- `5c1b790f` feat(host): tell the chat when a turn goes silent
- `c72d0486` feat(host): stop a turn stuck repeating the same tool call
- `21f1f815` feat(host): versioned bot instructions with author, reason and diff
- `5ef47f72` feat(host): watch triggers wake a routine only when its target changes
- `45400afc` feat: bot secrets for API calls and commands
- `f5200bef` feat(host): egress policy for requests to untrusted URLs
- `bcd10116` feat(android): search models by name, id and description; release tags follow the host version
- `649363a9` docs: add the October 2026 upstream integrations guide
- `e24dc929` chore(android): drop JVM crash logs committed by mistake
- `18fa230d` fix: phones load the roster right after pairing
- `7e7b7fcd` fix(android): scroll to the newest message after layout, not during it
- `dce33a1e` test: Windows paths in desktop update fixtures; alert subtitle leads the message
- `4f5317c8` fix(desktop): roster empty state, project labels and metadata line
- `923e0003` fix(android): compile the merged blocks and their tests
- `476aa4d4` feat(android): several computers per phone, removed computer, notification subtitle and live Bots tile
- `e229b4dd` feat: roster search, filters, project groups and previews (block C)
- `ad292f78` feat(android): accent, theme, density, haptics and sheets (block B)
- `d0d5be9f` feat(android): four destinations, navigation rail and split view (block A)
- `774da120` feat: merge upstream improvements into main
- `1e8f1cb8` docs: add Android and desktop implementation plans
- `38b763f9` build: bump version to 2.20.0
- `ed6a070b` feat: integrate upstream agent and screen improvements
- `e60772d2` docs: add bot avatar viability plan
- `ed0ab1f8` docs: add Android and desktop UI improvement plan
- `c9031e85` test(android): validate constellation on emulator
- `3567515f` feat(android): add the constellation bot view
- `c562228b` refactor(android): share bot roster actions
- `72075c9c` feat(android): port the bot memory screen (2.18/2.19)
