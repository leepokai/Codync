# Accounts, device approval and SSH

The iOS app uses the official ClerkKit SDK (1.5.6). The desktop app talks to Clerk's
Frontend API directly, the way ClerkKit does on native apps (`apps/desktop/src/main/account.ts`):
a device token kept encrypted with Electron `safeStorage`, OAuth in the system browser.

## Configuration

The iOS app uses `apps/ios/App/AccountSession.swift`. Public keys and cloud URLs come from `apps/shared/Config/<env>.plist`, copied into the bundle as `AccountConfig.plist` by `project.yml`; `iOS Dev` (`DevDebug` / `DevRelease`) selects dev and `iOS` (`Debug` / `Release`) selects main. The desktop app reads the same plist through `node apps/desktop/tools/account-config.mjs <env>` (run by `npm run dev` and `npm run dist:*:<env>`), which writes `resources/account-config.json` (`CODYNC_CLERK_PUBLISHABLE_KEY` / `CODYNC_CLOUD_URL` override it). See [environments](environments-and-deployment.md).

Confirm Apple and Google sign-in, Native API and the native app registrations in the intended Clerk instance. The desktop app uses `com.pokai.Codync`, iOS uses `com.pokai.Codync.ios`, with their matching `://callback` URLs. Dashboard configuration and actual OAuth consent must be verified independently of the checked-in plist. Do not bundle Clerk secret keys.

### Desktop Sign in with Apple

The welcome screen (`apps/desktop/src/renderer/views/AccountWelcomeView.tsx`) offers
**Continue with Apple** and **Continue with Google**; **Settings → General → Account**
has Sign out.

On the desktop, `AccountService.signIn('apple')` starts Clerk's `oauth_apple` sign-in
and opens its redirect in the system browser, which returns to
`com.pokai.Codync://callback` (`apps/desktop/src/main/auth.ts`). This uses Clerk's
Apple Services ID and needs no native Sign in with Apple entitlement. iOS continues
using Clerk's native Apple authorization.

The production Services ID `com.pokai.Codync.signin` is associated with primary
App ID `com.pokai.Codync.ios` in Apple Developer. Keep that association: Apple's
web and native authorization must represent the same identity rather than being
joined by email. The desktop app's `com.pokai.Codync://callback` must also remain allowed
in Clerk. See [Apple's web configuration guide](https://developer.apple.com/help/account/capabilities/configure-sign-in-with-apple-for-the-web/)
and [Clerk's Apple OAuth guide](https://clerk.com/docs/guides/configure/auth-strategies/social-connections/apple).

The development instance uses the same custom credentials (Services ID, team
`7FUM8A8H72`, key `8S76D7ADWU`) since 2026-10-03, with
`sunny-mollusk-8651.clerk.accounts.dev` and its `/v1/oauth_callback` added to the
Services ID. Clerk's shared Apple credentials belong to Clerk's team, so with them
the computer's web sign-in and the iPhone's native sign-in got different Apple user IDs
and became two Clerk users; don't switch development back to shared credentials.

To check cross-device discovery, sign in with the same Apple Account on both
devices of one environment (Dev ↔ Dev, production ↔ production) and verify that
Clerk has one user ID. Then verify that the computer joins the account and the phone
requests access with the existing approval flow. Hide My Email does not change the
device approval requirement.

The desktop app's Apple sign-in and production cross-device identity matching
still need to be checked on a release build.

### iOS Sign in with Apple

The welcome, pairing and account screens offer **Continue with Apple** alongside
Google, with matching headline typography and button sizing. Apple's native
authorization is presented by Clerk. Adding another account lets the user choose
either provider. `AccountSession.signIn(provider: .apple)` calls
`clerk.auth.signInWithApple()`; Clerk owns Apple's credential exchange, new-user
transfer, session activation and persistence. Cancellation is silent and
incomplete sign-in/sign-up results remain explicit errors.

Enable Apple for sign-up and sign-in in both Clerk environments, and register
`7FUM8A8H72.com.pokai.Codync.ios` under Native applications. Enable the App ID's
`APPLE_ID_AUTH` capability as a primary App ID and retain
`com.apple.developer.applesignin: [Default]` in `iOS.entitlements`. Regenerate
provisioning profiles after adding this entitlement. Native setup is described
in [Clerk's Apple sign-in guide](https://clerk.com/docs/ios/guides/configure/auth-strategies/sign-in-with-apple).

On 2026-09-30, Apple sign-up/sign-in was enabled in Codync's development and
production Clerk instances. Production's connection required custom credentials:
Services ID `com.pokai.Codync.signin`, primary App ID `com.pokai.Codync.ios`,
domain `clerk.codync.dev` and return URL
`https://clerk.codync.dev/v1/oauth_callback`. The Sign in with Apple key is stored
privately outside the repository and supplied only to Clerk. Never add its P8
contents to app resources, logs or documentation.

Apple Private Email Relay also has the production Clerk sender domain
`clkmail.codync.dev` and sender
`bounces+115655512@clkmail.codync.dev` registered as email sources. Registration
does not replace an actual delivery test to an Apple relay address.

Account storage and computer approval still use the Clerk user ID, regardless of
login provider. Apple **Hide My Email** can create a different Clerk user from an
existing Google account; do not merge users by unverified email or assume both
providers reach the same computers. Test new-user registration with Hide My Email,
returning-user login, cancellation, restoration, account switching and sign-out
on a real device before release. Build success and enabled dashboard settings do
not prove that an Apple authorization completed successfully.

QR pairing does not require matching email addresses or even a signed-in computer.
The iPhone's scan step always offers **Skip**, including when signed out or when
the account has no computers. Skipping finishes onboarding and opens the main
tabs; this choice survives relaunches and account changes. The empty Bots screen
offers **Computers** to pair later. Skipping while adding a computer from settings
only closes that pairing sheet. It does not grant access to a computer or change
the account approval flow. **Start over** resets the onboarding choice.

The desktop app's menu bar **Settings → Reset all data…** (confirmed in the window) is
the computer's start over: it removes this computer from the account, revokes every
paired device, signs out, uninstalls the host, deletes its data folder and the app's
settings, and relaunches into the welcome screen. The host comes back with a new
identity, so phones see the old computer as **No access** (if they were connected)
or as an older copy once the new one is reachable, and offer to remove it; pair
again or ask for access to reach the new one.

On the computer, open Codync in the menu bar and choose **Pair iPhone…**, then scan the
code on the phone (or paste its `codync://pair` link). The host approves the phone's
device key; the phone saves the computer in its current account context. This
works when the phone uses Apple Hide My Email and the computer uses Google. Automatic
account discovery, in contrast, requires the same Clerk user ID on both devices.

On 2026-09-30, the production dashboard confirmed a successful Apple registration
with a private relay address. The subsequent native `/v1/devices` and
`/v1/computers` requests failed with 401 because the Worker's `CLERK_SECRET_KEY`
contained the dashboard's abbreviated value. The complete key was verified
against production JWKS and stored in the Worker. Relaunching the Release build
on the physical iPhone restored the Apple session, and both endpoints returned
200 without warnings. The cloud API regression tests
cover native device registration and computer listing with both an absent email
claim and an Apple relay address.

## Flow

On iPhone, `apps/ios/App/AccountSession.swift` configures Clerk once. Continue with Google calls `clerk.auth.signInWithOAuth(provider: .google)`
directly, without the Clerk Account Portal intermediary. Google authorization
uses the SDK's system browser authentication session. Clerk handles new-user
transfer, callback validation, session restoration and Keychain storage.
Incomplete sign-in/sign-up results are reported explicitly; additional MFA or
required profile fields need a separate continuation UI if enabled later. Browser cancellation is silent;
network failures leave the user in the custom account menu with a retryable
error. Log out calls Clerk's sign-out API. The desktop app follows the same
flow over the Frontend API (`apps/desktop/src/main/account.ts`).

The custom account menu displays the authenticated email and avatar when
available. `AccountSession.sessionToken()` (desktop: `AccountService.sessionToken()`, used by `cloud.ts`)
hands the session JWT to the Codync cloud client. A Clerk session never authorizes a computer by itself: each
computer approves each device after comparing a 6-digit code
([remote relay protocol](../reference/remote-relay.md) §4.2). Conversations are not uploaded.

## Verification

- Build the iOS scheme after `xcodegen generate --spec apps/project.yml`, and run the
  desktop app with `npm run dev` in `apps/desktop/`.
- Open the account menu from either sidebar layout; test arrows, Return, Escape
  and clicking outside the panel.
- Sign in with Apple and Google test users, verify the avatar/email, restart the app and
  verify session restoration, then log out and verify the anonymous state.
- Cancel the browser flow and verify no error; retry with networking unavailable
  and verify that the application stays usable.

Dashboard creation and the end-to-end Apple/Google flows require the user's browser
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
`apps/shared/Config/<env>.plist`. The repository configuration does not prove
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

## Desktop: computers, approvals and SSH

The desktop app attaches this computer's host and its SSH computers over loopback; they
follow into whichever account is active. The account's other computers are not reached
directly (the encrypted channel wasn't ported to the desktop app), so their
"Ask for access" rows show status only. See [desktop app](../architecture/desktop-app.md).

**Computers & devices** (Settings) shows, for this computer and each SSH computer:
*Reach from anywhere* (`setCloud`, on/off; the host's build decides which cloud),
*Add to account* (claim: `POST /v1/claims` → loopback `claimSign` → complete) and
*Remove from account* (`unclaim`), a pairing QR, and the authorized devices with revoke.
A device asking for access raises the menu bar dot and opens the approval sheet with the
6-digit code; Approve stays disabled until the device revealed its code.

SSH computers use the system OpenSSH (`apps/desktop/src/main/ssh.ts`): `ssh -G` to
resolve the target, `ssh-keygen -F` against `~/.ssh/known_hosts` and
`~/.codync/ssh_known_hosts`, a fingerprint confirmation on first contact (no proxy),
`codync-host info --json` for the identity and loopback token, then
`ssh -N -L 127.0.0.1:<free port>:127.0.0.1:<remote port>` with keepalive and backoff.
A changed host key or a different computer ID blocks the connection. Sign-in is key or
ssh-agent only (`BatchMode`: nothing prompts for a password or passphrase); a refused key
stops with a message instead of retrying. The remote command searches Homebrew,
`~/.local/bin` and the Mac app bundle for `codync-host` too, since `sh -l` doesn't read
`~/.zprofile`. At launch the app kills tunnels a crashed or force-quit copy left behind
(`pkill` on the `.codync/ssh_known_hosts` argument only Codync's tunnels carry).

## Verification boundaries

Build with normal simulator signing for Clerk Keychain access. Test Google consent, cancellation, restoration, two-account switching, per-account cache isolation and sign-out on real test accounts. Build success cannot verify Clerk Dashboard state.

For SSH, verify first-contact fingerprints and changed-key rejection. With ProxyJump/ProxyCommand, establish trust in a terminal first. The app checks that the local forwarding listener belongs to its SSH process. The desktop app is not a phone-to-SSH gateway.

Use [Cloudflare testing](cloudflare-testing.md) for remote account approval and relay acceptance.
