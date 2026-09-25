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
   `apps/macos/Resources/AccountConfig.plist` (`clerkPublishableKey`), next to
   `cloudURL`, the Codync cloud the app talks to (staging for now). A development
   launch can override them with `CODYNC_CLERK_PUBLISHABLE_KEY` and `CODYNC_CLOUD_URL`.

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
available. `AccountSession.sessionToken()` hands the session JWT to the Codync
cloud client. A Clerk session never authorizes a computer by itself: each
computer approves each device after comparing a 6-digit code
([remote-relay-spec.md](remote-relay-spec.md) §4.2). Conversations are not uploaded.

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
`apps/ios/Resources/AccountConfig.plist`. The repository configuration does not prove
that the corresponding Clerk Dashboard registration has been completed.

To retain several accounts at once, enable multi-session support in the Clerk
instance. The UI reads `authConfig.singleSessionMode`: with multi-session enabled,
it lists active SDK sessions and switches with `auth.setActive(sessionId:)`; with
single-session enabled, it asks the user to sign out before using another account.
Signing out passes the current session ID, rather than signing out all accounts.

Computers, device keys and caches are partitioned by the Clerk user ID on this
iPhone (`SharedStore.Context`). Computers paired with a code while signed out stay
in the local context and are not claimed by a newly signed-in user. Each account
context gets its own `AccountStore`, retired when changing accounts; signing out
erases that account's computers, device and push keys and caches from the iPhone.
Signed in, the Computers screen also lists the account's computers from the cloud;
asking one for access shows the 6-digit code to compare on the computer.

Verify with two real Google accounts: add both, switch, cancel OAuth, restart,
check each account's pairings, sign out just one, and verify the other remains.
A simulator build verifies compilation, not the Dashboard settings or OAuth flow.

## Mac: computers, approvals and SSH

The Mac keeps one `AccountStore` per account context too (`HostController`). This Mac's
host and SSH computers are attached over loopback and follow into whichever account is
active; the account's other computers are reached over the encrypted channel with the
Mac's own device key. Log out erases the account's keys and caches, as on iPhone.

**Computers & devices** (account menu) shows, for this Mac and each SSH computer:
*Reach from anywhere* (`setCloud`, using the app's `cloudURL` when the host has none),
*Add to account* (claim: `POST /v1/claims` → loopback `claimSign` → complete) and
*Remove from account* (`unclaim`), a pairing QR, and the authorized devices with revoke.
A device asking for access raises the menu bar dot and opens the approval sheet with the
6-digit code; Approve stays disabled until the device revealed its code.

SSH computers use the system OpenSSH (`apps/macos/App/SSHTunnel.swift`): `ssh -G` to
resolve the target, `ssh-keygen -F` against `~/.ssh/known_hosts` and
`~/.codync/ssh_known_hosts`, a fingerprint confirmation on first contact (no proxy),
`codync-host info --json` for the identity and loopback token, then
`ssh -N -L 127.0.0.1:<free port>:127.0.0.1:<remote port>` with keepalive and backoff.
A changed host key or a different computer ID blocks the connection. Debug builds run
`SSH.selfCheck()` at launch (argv, `ssh -G` parsing, validation).
