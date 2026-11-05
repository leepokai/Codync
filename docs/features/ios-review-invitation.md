# iPhone rating invitations

The iPhone asks StoreKit for a review about a second after the fifth prompt it sent reaches the
computer (or the relay mailbox, while the computer is offline), over whichever screen is open.
It never requests on launch, pairing, sign-in, or foregrounding.

## Eligibility

- Five prompts from this iPhone that reached the computer or the relay mailbox. Voice calls don't count.
  A resend of a prompt that never got there counts once. The mailbox counts on arrival: one that then fails
  or expires counts again when resent, and taking one back doesn't subtract, so the ask can come a prompt early.
- At least 120 days between attempts, two attempts per rolling 365 days, one per version,
  and five more prompts since the previous attempt.
- A prompt sent while the app is in the background (a shortcut) doesn't ask, and neither does one the app can't ask on
  within five seconds; the next prompt does.
- StoreKit may suppress the prompt, and never shows it in TestFlight. Persist an attempt before
  calling it regardless; there is no event claiming it appeared or that someone rated the app.

The device retains the prompt count and request history; a history from before 2.39.1 keeps its requests and
starts counting prompts at zero. This is device-only StoreKit presentation
policy: no host API or compatibility minimum changes, and no sentiment analysis or rating pre-screen
is used.

Settings offers **Rate on the App Store** to everyone, opening Apple's write-review page,
and an independent **Report an issue** link to GitHub. Neither is gated on satisfaction.
Only StoreKit's own prompt appears automatically.

## Analytics

Use the existing iPhone PostHog client and opt-out switch. See the event contract in
[analytics events](../reference/analytics-events.md). For any rating-count trend use aggregate
App Store Connect reports; individual review conversion is unavailable. Never treat a StoreKit
attempt or a manual link tap as a submitted rating.

## Testing

E2E flows that send through the composer launch with the `-reviewInvitationOff` argument
(`apps/maestro/queue.yaml`; the dash is what makes UserDefaults read it): the system's window would cover what they
tap. `apps/maestro/review-invitation.yaml` checks the request itself: no window at the fourth prompt, StoreKit's after
the fifth.

## Platform scope

The iPhone uses StoreKit. The Android app has no invitation until it is on Google Play (the owner's call,
2026-11-01): its Settings has **Rate on Google Play** and **Report an issue**, and the invitation comes with the
Play release, through Play's in-app review API and this same policy. The desktop app invites a GitHub star at the
same fifth prompt instead (`apps/desktop/src/renderer/lib/star-ask.ts`); terminal settings have no store review
capability and keep their behavior.
