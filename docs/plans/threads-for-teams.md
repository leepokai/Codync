# Threads, kept for the team plan

Reply threads (Slack's "reply in thread") gave way to quote replies in 2.20.1: a reply is now a
message in the chat that quotes the one it answers ([groups and replies](../features/groups-and-replies.md)).
The team plan may bring threads back, so this page records what they were and where their code is.

## Getting the code back

Commit `ecc955018da2` (on `dev`) is the last one with threads; the commit that removed them is the
last one to touch the iPhone's deleted `RepliesView.swift`.

```sh
gone=$(git log -1 --format=%h origin/dev -- apps/ios/Kit/Sources/CodyncUI/Thread/RepliesView.swift)
git show ecc955018da2:docs/features/groups-and-threads.md      # the full design
git show ecc955018da2:host/src/agent/bot/session.rs            # one file as it was
git show "$gone" -- host/ apps/                                # everything the removal changed
git checkout ecc955018da2 -- apps/ios/Kit/Sources/CodyncUI/Thread/RepliesView.swift   # restore a deleted file
```

Threads plugged in where quote replies live now: host `store/model.rs` `Lane` and
`agent/bot/session.rs` `slot`, TUI `app/mod.rs` `reply`, desktop `ThreadView.tsx` `replyTo`, iPhone
`ThreadView.swift` `replyingTo`, Android `ChatScreen.kt` `replyId`.

## What threads were

- A thread hung off one main-chat message (the *root*); entries carried `threadId = <root id>`.
  Threads were flat: a reply couldn't start one.
- **Sessions:** in a bot's own chat each thread had its own ACP session. The first reply forked the
  chat's session (`session/fork`, keeping the parent's frozen instructions so the prompt cache was
  shared); an agent that couldn't fork started fresh, told the root and the 12 lines before it
  (`thread_intro`). Nothing said in a thread reached the main session.
- **Groups:** a thread was its own room. Room epochs were keyed by lane (`group/root`), members saw
  `[Group chat: "…", in a thread - …]` and the root line, and Stop ended every room of the group.
- **Read state:** the main chat and each thread were read separately (kv `read.<root>`); a bot's
  badge was the sum, and `markRead` took `threadId` or `all`.
- **Summaries:** the host kept `data.thread = {count, lastAt, authors, unread}` on each root, which
  clients showed under it ("3 replies · 1 new").
- **Routines** ran in a thread of their own (`routine-<run>`, the run's notice as root); the result
  was copied into the main chat as `routine-result-<run>`. Runs now talk in the chat from their own
  session (`routines::session_slot`).
- **Wire:** `send.threadId`, `thread {botId, rootId}`, `markRead.threadId`, entry `threadId`, bot
  `workingThread`, analytics `message_sent.in_thread` and the `thread` screen.

Schema migration 5 (`host/src/store/schema.rs`) merged every thread into its chat in time order;
each thread's first user reply got a `replyTo` quoting its root.

## Session fork, as it was implemented

Threads were the only user of ACP `session/fork` (a draft RFD, "Session Fork"); since 2.20.1 Codync
never forks. The code is in `host/src/agent/bot/session.rs` at `ecc955018da2`: the fork branch of
`ensure_session` (lines 200–236) and `thread_intro` (lines 62–95).

- **Capability.** At `initialize`, `launch.rs` set `SessionCaps.fork` when the agent advertised
  `agentCapabilities.sessionCapabilities.fork` as an object.
- **When it forked.** `ensure_session(slot)` for a slot (a thread root) with no session of its own yet,
  while the bot had a main session and the agent could fork. Routine runs went through the same path
  (their slot was the `routine-<run>` thread), so with a forking agent every run started as a copy of
  the main chat and got the thread intro quoting its "Routine · …" notice. That was a side effect, not a
  design; runs now start fresh.
- **The request.** `session/fork` with the usual session params (`cwd`, `mcpServers`, `sessionId` = the
  main session, `additionalDirectories` for the memory folders) plus the **main session's** frozen
  instructions (`snapshot(main)`): Claude's `_meta` (`claude_meta`), ACP `systemPrompt` (append), or nothing
  for agents that take the profile in their first message. Same instructions, same cached system prompt.
  The working line read "Opening the thread…"; the request timed out like the others (`SESSION_TIMEOUT`, 180 s).
- **After a fork.** The new session was marked loaded and stored for its slot (kv
  `lane.session.<bot>@<bot>/<root>`, in memory `thread_sessions`); its config options were noted (models,
  Fast mode); `context::adopt` gave it the parent's snapshot instead of a re-render; `memory::opened`
  recorded that it already held memory as the parent had it, so only later changes arrived as deltas; for
  Claude, `applied_system` was set so no reload followed.
- **What the agent was told.** The next prompt began with the intro (`compose`, after the memory notice):
  `[Thread] The user opened a thread on this message from your main chat — <who>: "<root>". This thread is
  its own branch: it starts from everything said so far, and nothing said here reaches the main chat. Just
  continue the conversation; there's no need to mention this.`
- **Without a fork** (no capability, an error or a timeout: "session/fork failed; the thread starts
  fresh"): `session/new`, and the intro carried the root and the messages among the 12 entries before it
  (the user's and final replies, 1500 bytes each). A compaction in a thread set the fresh intro again, so the summarized
  session still knew what the thread was about.
- **Lifetime.** Thread sessions loaded like any other, closed when idle; a reconfigure that restarted the
  agent forgot all of them (`thread_roots`, `set_session(Some(root), None)`).
- **Verified** (docs/design/bot-memory.md, 2026-10-18 run): a thread forked from a chat knew a memory card
  saved just before it and one saved earlier. The same doc had found that a fork carried no `_meta` (Codync
  sent none and the adapter forwarded none); that is why the fork sends the parent's instructions. The same doc planned starting a thread from a handoff instead
  of a fork when the main chat is nearly full; the fork branch never checked that.

## Where it lived (at `ecc955018da2`)

### Host

- `store/model.rs`: `Lane { chat, thread }`, `Lane::in_thread`, `Lane::key` (`chat/root`), `lane_key`,
  `ReadScope { All, Chat, Thread }`, `Entry.thread_id`.
- `store/entries.rs`: the `thread_id` column, `thread()` (a thread's replies), `thread_summary()`,
  per-lane `messages_after` / `last_spoke`, main-chat-only `history` and `bot_conversation`.
- `store/bots.rs`: `mark_read` seeding thread read marks, `mark_thread_read`, `mark_all_read`,
  `unread(scope)`, `unread_roots`. `store/schema.rs`: migration 4 added `thread_id` and `entries_thread`.
- `hub.rs`: `mark_read(scope)`, `update_thread_summary` / `refresh_thread_summary`; `hub/sync.rs`: `workingThread`.
- `api/bots.rs`: the `thread` method, `send.threadId` (root checks), `markRead.threadId | all`.
- `agent/bot/session.rs`: `slot(lane)`, `thread_sessions`, `thread_roots`, `thread_intro`, the
  `session/fork` branch of `ensure_session`; `mod.rs` / `launch.rs`: `SessionCaps.fork`, the
  `turn.inflight` value `started\troot`; `queue.rs`, `turn.rs`, `compaction.rs`: resuming in a thread
  lane, the intro in `compose`, the intro again after a compaction.
- `chat/group.rs`: thread rooms; `routines/runner.rs`: runs in thread lanes and the copied result;
  `chat/memory/{tools,notices}.rs`, `market/requests.rs`: thread lanes, search `inThread`;
  `mcp.rs`: the unused `memoryLane`; `analytics/{observe,reported}.rs`: `in_thread`, `Screen::Thread`.
- Terminal UI: `tui/app/mod.rs` (`thread` state, lane filter, `bot#root` drafts, `bot/root` read
  scopes, jumping to a card waiting in a thread), `app/navigate.rs` (`open_thread`), `app/input.rs`,
  `app/events.rs` (`After::Thread`, refetch on rewind), `app/model.rs` (`working_thread`,
  `works_in(thread)`), `app/exchange.rs` (`group(.., main)`), `view/transcript.rs` (`thread_line`,
  `thread_summary`, the "N replies" header), `view/chat.rs`, `view/trace.rs`, `view/help.rs`, `app/analytics.rs`.
- Tests: `chat/group/tests.rs` `threads_stay_out_of_the_main_chat_and_summarize_on_their_root`,
  `tests/files_e2e.rs` (files in a thread), `tests/routines_e2e.rs` (a run's thread continues in its session).

### Desktop (`apps/desktop/src/`)

- `renderer/views/thread/RepliesView.tsx`: the panel or sheet with the root, "N replies", the replies
  (`buildChat(replies)`), a thread-only working line, and a `Composer` / `TraceView` with `thread`.
- `renderer/views/thread/ThreadView.tsx`: `openThread` state, `openThreadOn` / `closeThread`, the
  replies panel in the `ChatSidePanel` slot (resizable, `repliesPanelWidth` pref in `lib/prefs.ts`),
  a sheet when narrow.
- `renderer/views/thread/ChatRows.tsx`: `ThreadChip` (reply avatars, "N replies", "N new" or time),
  the "Reply in thread" footer and context menu items.
- `Composer.tsx` and `TraceView.tsx`: the `thread` prop (drafts, working state, placeholder, trace per lane).
- `reading.ts`, `store/bot-sync.ts`: read scopes `{botId, thread}`; `chat-items.ts`: `buildChat(.., grouped)`;
  `store/bot-mirror.ts`: `chat()` without threads, `replies()`, `currentThinking(.., thread)`;
  `store/bot-store.ts`: `send(.., thread)`, `retry`, `loadThread`; `client/host-client.ts`: `thread()`.
- `shared/models.ts`: `workingThread`, `isWorkingIn(.., thread)`, `ThreadSummary`, `Entry.threadId`;
  `shared/mirror-window.ts`, `shared/composer-drafts.ts` (`[bot, thread]` keys, `codync-drafts-v1-…`),
  `views/call/voice-call.ts`, `shared/analytics.ts`, `views/thread/thread.css` (`.thread-chip`, `.replies-*`),
  `demo/fake-host.ts` and `demo/story.ts` (seeded threads, `summarize()`), `tools/composer-check/main.tsx`.

### iPhone (`apps/ios/Kit/Sources/`)

- `CodyncUI/Thread/RepliesView.swift`: the thread sheet (root, "N replies", replies, working line,
  composer with `threadId`, its own read scope, export sheet and trace) and `ThreadTarget`.
- `CodyncUI/Thread/ThreadView.swift`: `openThread`, `closeThread()`, `.codyncSheet(item: $openThread)`;
  `ChatRows.swift`: `ThreadChip`, "Reply in thread", `currentThinking(_:thread:)`; `Composer.swift`: `thread`.
- `CodyncUI/Thread/ReadingConversation.swift`, `Store/BotStore+Actions.swift`, `+Connection.swift`:
  `ReadingScope(botId, thread)`, `markRead(_:thread:)`, `loadThread(_:root:)`.
- `CodyncUI/Store/BotStore.swift` (`chat()`, `allEntries()`, `replies`), `+Sync.swift` (paging floors,
  voice), `+Sending.swift` (`[botId, thread]` drafts, `threadId` on every send path), `FileDownloads.swift`,
  `Thread/TraceView.swift`.
- `CodyncKit/Models/Models.swift` (`threadId`, `ThreadSummary`, `workingThread`), `EntryData.swift`
  (`thread`), `Client/HostClient.swift`, `HostTransport.swift`, `ChannelTransport+Mailbox.swift`,
  `Analytics/AnalyticsEvent.swift` (`Screen.thread`).

### Android (`apps/android/`)

- `app/.../ChatScreen.kt` `RepliesView` (the thread sheet), `ChatRows.kt` `ThreadChip` and
  "Reply in thread", `AppState.kt` `thread`, `AppStoreChat.kt` (`openBot(id, thread)`, `ReadingScope`),
  `AppStoreDrafts.kt` / `DraftBook.kt` (`ComposerDraft.lane(botId, threadId)`), `MirrorDatabase.kt`
  (`entries.thread_id`, `entries_lane`), `AppStoreFiles.kt`, `TraceView.kt`, `MessageComposer.kt`, `AnalyticsEvent.kt`.
- `core/.../Conversation.kt` (`threadId`, `ReplySummary`), `HostClient.kt` (`workingThread`),
  `EncryptedChannel.kt`, `Voice.kt`, `CallOperator.kt`.
- Tests: `RepliesPopoverTest`, `LiveThreadTest`, and the thread parts of `LiveDraftTest` and `DraftBookTest`.

## Worth reusing for teams

- **Forked sessions** are the cheap way to give a topic its own context: the fork shares the chat's
  cached prompt and history, and nothing flows back. A team member working on a sub-task could run
  in one, as a thread did.
- **Lanes** (`chat` + an optional sub-conversation) were the one type the queue, rooms, read state
  and traces keyed on; adding the field back to `Lane` is where a revival starts.
- **Rooms per lane** let a group hold several conversations at once without their room turns ending
  each other.
