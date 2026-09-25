# macOS account authentication

The macOS app uses the official ClerkKit SDK (1.5.6). The sidebar's account
panel is a custom SwiftUI overlay, independent of the system authentication browser.
The footer displays **Account**, never the Mac user's local name.

## Dashboard setup

1. Create a Clerk application named **Codync** and enable Google sign-in.
2. Enable the Native API under **Native applications**.
3. Register the native application for team `7FUM8A8H72` and bundle identifier
   `com.pokai.Codync`, following Clerk's Apple/native configuration instructions.
4. Allow the native callback `com.pokai.Codync://callback` where Clerk's dashboard
   requests allowed redirect URLs.
5. Copy only the **publishable key** into
   `apps/macos/Resources/ClerkConfig.plist` (`publishableKey`). A development
   launch can override it with `CODYNC_CLERK_PUBLISHABLE_KEY`.

The checked-in public configuration uses the Codync development instance
`sunny-mollusk-8651.clerk.accounts.dev`. Native API is enabled, the Apple app
is registered, and `com.pokai.Codync://callback` is allowlisted. Google and
email sign-in are enabled. Production deployment needs its own Clerk instance
and publishable key. Never put a Clerk secret key in the Mac bundle or this file.

## Flow

`apps/shared/AccountSession.swift` configures Clerk once per app. Continue with Google calls `clerk.auth.signInWithOAuth(provider: .google)`
directly, without the Clerk Account Portal intermediary. Google authorization
uses the SDK's system browser authentication session. Clerk handles new-user
transfer, callback validation, session restoration and Keychain storage.
Incomplete sign-in/sign-up results are reported explicitly; additional MFA or
required profile fields need a separate continuation UI if enabled later. Browser cancellation is silent;
network failures leave the user in the custom account menu with a retryable
error. Log out calls Clerk's sign-out API.

The custom account menu displays the authenticated email and avatar when
available. A Clerk session does not replace the local host pairing token,
authorize remote host access, upload conversations, or migrate local data.

## Verification

- Build the macOS scheme after `xcodegen generate`.
- Open the account menu from either sidebar layout; test arrows, Return, Escape
  and clicking outside the panel.
- Sign in with a test Google user, verify the avatar/email, restart the app and
  verify session restoration, then log out and verify the anonymous state.
- Cancel the browser flow and verify no error; retry with networking unavailable
  and verify that the application stays usable.

Dashboard creation and the end-to-end Google flow require the user's browser
consent; compilation alone does not verify those steps.

References:
- https://clerk.com/docs/ios/getting-started/quickstart
- https://github.com/clerk/clerk-ios

## iOS account switching

The iOS app now uses the same ClerkKit dependency and shared AccountSession.
Its top-left button opens Accounts; Computers & settings is a separate destination
inside that sheet. The iOS native application must be registered in the same Clerk
instance with bundle ID `com.pokai.Codync.ios` and its own callback
`com.pokai.Codync.ios://callback`. The iOS public configuration is in
`apps/ios/Resources/ClerkConfig.plist`. The repository configuration does not prove
that the corresponding Clerk Dashboard registration has been completed.

To retain several accounts at once, enable multi-session support in the Clerk
instance. The UI reads `authConfig.singleSessionMode`: with multi-session enabled,
it lists active SDK sessions and switches with `auth.setActive(sessionId:)`; with
single-session enabled, it asks the user to sign out before using another account.
Signing out passes the current session ID, rather than signing out all accounts.

Pairings and caches are partitioned by the Clerk user ID on this iPhone. Local
pairings keep their original storage namespace and are not automatically claimed
by a newly signed-in user. Each BotStore captures a fixed storage context, and is
retired when changing accounts. This is client-side separation, not server-side
ownership enforcement. Cloudflare device discovery, per-device grants, account-aware
push revocation and Keychain migration remain work in the architecture plan.

Verify with two real Google accounts: add both, switch, cancel OAuth, restart,
check each account's pairings, sign out just one, and verify the other remains.
A simulator build verifies compilation, not the Dashboard settings or OAuth flow.
