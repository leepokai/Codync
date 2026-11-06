# Topic 8: mini-app and check-in fixes (commit b6bb8228)

This topic was written against a branch that did not have the mini-app and check-in work. After the base update (topic 1)
the branch has it, together with the fixes upstream made after `b6bb8228`. This report says, item by item, what the merged
base already does (with where it is proved) and what this topic added.

## Item by item

| Item | State on the merged base | Evidence |
|---|---|---|
| 1. A check-in is not a visible chat message; `NO_UPDATE` is not shown or notified | Done upstream. The check-in's message is internal (`from: "proactive"`, `internal: true`; `dispatch_bot_turn`, `start_turn`), the turn buffers its output and publishes it only when it is useful (`agent/bot/proactive.rs`), a reply that is exactly `NO_UPDATE` (or a pass) is silent and raises no alert (`silent_check_in` in `finish_turn`); clients hide internal entries (`isInternalEntry` on desktop, `visibleEntries` on Android, `is_internal` in the terminal). | `agent/bot/proactive_tests.rs` (a useful final is revealed without its prompt or steps; `NO_UPDATE` never becomes visible, streaming or sent explicitly; useful sends are published once; queued check-ins never coalesce with a person's messages) and `agent/bot/proactive.rs` unit tests |
| 2. Android WebView opens anything outside `https://codync-app.invalid` in the system browser; the bridge serves only the main page of that origin | Done upstream: `MiniAppWebClient.shouldOverrideUrlLoading` routes by `miniAppNavigation` (in-app, browser, blocked), `onPageStarted` stops a POST navigation that left the origin, `shouldInterceptRequest` answers `403` outside the virtual origin and serves the bridge script only to sub-frames. | `MiniAppWebClient.kt`; `MiniAppNavigationTest` in `:core:test` (part of the Android gate) |
| 3. `fetch` in the bridge loses non-text bodies | Done upstream: `miniAppFetchBody` handles the body, and a `Request` from another frame throws a clear `TypeError` instead of losing its body. | `MiniAppBridgeScript.kt`; `app/src/test/js/miniapp-bridge.test.mjs`, run with `node --test` for this topic: 9 passed, 0 failed (non-text bodies reject clearly before any host request; a `Request` body is rejected, not discarded; GET/HEAD with a body is refused) |
| 4. Host: `needless_borrow` in `proactive.rs`, `kv_delete(...).ok()` swallowing errors, `appRequest` tests | Done upstream (`43aafe79`): the cleanup failure is logged with `tracing::warn!`, clippy is clean, and `apps_bridge_tests.rs` covers pages, data, bad capabilities and paths with `..`. | `cargo clippy` and `api::apps_bridge::tests` |
| 5. Desktop main process opens `window.open` URLs without checking the scheme | Done upstream: `main/artifacts.ts` opens only `https?` and `mailto` from a new window, and `app:openExternal` checks its scheme. | `apps/desktop/src/main/artifacts.ts`, `index.ts` |
| E2E: check-ins every hour, ledger, no visible message, no spurious notification | The ledger and the internal message are tested here; waiting an hour in a real host is not practical. | new test in `proactive.rs`, below |

## What this topic added

- **A defect of mine from topic 4, found while checking item 1:** a scheduled check-in that ran out of steps would have pushed
  a "Paused" alert for a message nobody sent. `turn_alert` now takes `check_in` and keeps a half-done check-in private; an
  error in a check-in is still told (it was before). Test: `only_finished_tasks_alert`.
- **A test of the ledger** (`proactive::tests::turning_check_ins_on_starts_the_ledger_without_saying_anything_and_the_first_one_stays_internal`):
  with check-ins on every hour, the first look only creates `proactive:<bot>` and sends nothing; an hour later the one
  message it sends is internal and from the scheduler. In this host there is no running bot to take it, so the check-in is
  not counted (`count: 0`) and the next tick asks again; the test says so.

## Not done, and why

- The real-host e2e over an hour: the scheduler's first check-in waits one full interval and the minimum interval is one
  hour. The pieces are covered by the tests above; nothing waited an hour.
- Nothing was exercised on a phone: the WebView's behavior is covered by `MiniAppNavigationTest` and the bridge's Node test, not seen. `shouldOverrideUrlLoading` and `shouldInterceptRequest` themselves (Android framework callbacks) have no test.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test -j 1 --bin codync-host -- --test-threads=1` | 859 passed, 0 failed, 1 ignored |
| Android, desktop | unchanged since topic 7 (Android gate and 392 desktop tests passed) |
