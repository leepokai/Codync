# iPhone rating invitations

The iPhone asks StoreKit for a review at a natural pause: after a locally sent bot interaction
finishes, its final result stays visible at the bottom of the conversation for three seconds,
and the person returns to Bots. It waits two more seconds with no interaction. It never
requests on launch, pairing, sign-in, or foregrounding. These are conservative initial
thresholds, not measured conversion optima.

## Eligibility and cancellation

- At least seven elapsed days since this feature first ran, activity on three different local
  calendar dates, and five completed, viewed interactions.
- At least 60 seconds continuously active before returning to Bots.
- At least 120 days between attempts, two attempts per rolling 365 days, one per version,
  and five additional completed/viewed interactions since the previous attempt.
- Navigation, touching the list, backgrounding, sheets, errors, incompatible versions,
  disconnected computers, running agents, or pending approvals cancel the opportunity.
  Cancellation does not consume the cooldown; it needs a new completed/viewed interaction.
- StoreKit may suppress the prompt. Persist an attempt before calling it regardless; there
  is no event claiming it appeared or that someone rated the app.

The device retains first-use time, three activity-day markers, counts, request history, and
up to 100 locally generated message nonces, valid for 30 days. Expired nonces are removed
on the next launch, foreground transition, or send. This is device-only StoreKit
presentation policy, using the host's existing turn IDs, statuses, and final-message flags.
No host API or compatibility minimum changes. Pending sends survive a relaunch; presentation
opportunities do not. Successful turns with notices are conservatively excluded, as are
voice calls, routine results, historical messages without a locally recorded send, failures,
and cancellations. Multiple messages in one turn count once. Reading means the result was
visible, not that the user approved it; no sentiment analysis or rating pre-screen is used.

Settings offers **Rate on the App Store** to everyone, opening Apple's write-review page,
and an independent **Report an issue** link to GitHub. Neither is gated on satisfaction.
Only StoreKit's own prompt appears automatically. All seven existing translations include
the settings label.

## Analytics

Use the existing iPhone PostHog client and opt-out switch. See the event contract in
[analytics events](../reference/analytics-events.md). Compare `review_opportunity` to
`review_requested`, grouped by `app_version`, and inspect cancellation reasons. For any
rating-count trend use aggregate App Store Connect reports; individual review conversion
is unavailable. Never treat a StoreKit attempt or a manual link tap as a submitted rating.

## Platform scope

The iPhone uses StoreKit. The Android app has no invitation until it is on Google Play (the owner's call,
2026-11-01): its Settings has **Rate on Google Play** and **Report an issue**, and the invitation comes with the
Play release, through Play's in-app review API and this same policy. Desktop Settings and help menu and terminal
settings have no store review capability and keep their behavior.

## Validation (2026-10-26)

- iOS package: 286 tests passed, including 13 invitation regression tests; iOS Dev simulator
  build passed. The three-reviewer pass confirmed and fixed obscured reading, expired nonce
  cleanup, and the missing independent issue-report link.
- Device Hub on a dedicated iOS 26.5 simulator: fresh launch had no rating prompt; version
  2.29.0 Settings exposed Rate on the App Store and Report an issue. UI testing was narrowed
  to this smoke check at the owner's request. An eligible StoreKit display and live PostHog
  ingestion were not verified; request semantics and analytics opt-out have unit coverage.
- Desktop: typecheck and 258 tests passed. User documentation: 18 pages and 88 links passed.
- Version-only host checks: formatting and Clippy passed; the broad suite progressed through
  many suites but timed out at `stop_ends_a_turn_whose_agent_ignores_the_cancel` after the
  hook's 300-second limit. Host source is unchanged. Android checks were not run; the owner
  explicitly excluded Android review invitations because the app is not published yet.
