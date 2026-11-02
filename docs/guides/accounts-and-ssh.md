# Accounts, device approval and SSH

The iOS app uses the official ClerkKit SDK (1.5.6). The desktop app talks to Clerk's
Frontend API directly, the way ClerkKit does on native apps (`apps/desktop/src/main/account.ts`):
a device token kept encrypted with Electron `safeStorage`, OAuth in the system browser.

## Recovering phone access

On iOS and Android, an unreachable paired account computer shows **Ask for access**
next to its connection error. A pending request reopens its code. If this phone
already has account approval, **Request new approval** explicitly replaces only
this phone's grants, then starts the normal matching-code approval flow. Opening
the sheet alone does not revoke anything. The computer must be online to answer;
requesting access cannot repair an offline host. Existing pairing data and local
addresses and color/route preferences survive renewal, and a saved pairing never counts as
approval of a new request.

Phone device lists do not offer **Remove** for computers in the signed-in account,
including older identities. Independently paired computers can still be removed
locally, and **Revoke this phone's access** remains available for account grants.
Android keeps the current pairing while another computer is being approved, then
switches its local connection only after approval; both account records remain.
Desktop account management remains the place to remove an account computer.
This is a phone recovery flow: the desktop manages the host/account and the TUI
connects to its local host; neither has this phone access-request screen.

## Configuration

The iOS app uses `apps/ios/App/AccountSession.swift`. Public keys and cloud URLs come from `apps/shared/Config/<env>.plist`, copied into the bundle as `AccountConfig.plist` by `project.yml`; `iOS Dev` (`DevDebug` / `DevRelease`) selects dev and `iOS` (`Debug` / `Release`) selects main. The desktop app reads the same plist through `node apps/desktop/tools/account-config.mjs <env>` (run by `npm run dev` and `npm run dist:*:<env>`), which writes `resources/account-config.json` (`CODYNC_CLERK_PUBLISHABLE_KEY` / `CODYNC_CLOUD_URL` override it). See [environments](environments-and-deployment.md).

Confirm Apple, Google and GitHub sign-in, Native API and the native app registrations in the intended Clerk instance. The desktop app uses `com.pokai.Codync`, iOS uses `com.pokai.Codync.ios`, with their matching `://callback` URLs. Dashboard configuration and actual OAuth consent must be verified independently of the checked-in plist. Do not bundle Clerk secret keys.

### GitHub sign-in

**Continue with GitHub** sits under Google on every sign-in surface (welcome, account menu, custom icon dialog; desktop Settings → General → Account). It is Clerk's built-in GitHub connection, run like Google: `oauth_github` through the system browser on the desktop, `clerk.auth.signInWithOAuth(provider: .github)` on iOS, `OAuthProvider.GITHUB` on Android. GitHub gives Clerk the account's verified email, so nothing in `cloud/` changes. The development instance can use Clerk's shared credentials; production needs a GitHub OAuth App (GitHub → Settings → Developer settings → OAuth Apps) whose callback URL is the one Clerk shows under SSO connections → GitHub → Use custom credentials, with its Client ID and secret pasted there. Enable it in production before a release with the button ships.

### Desktop Sign in with Apple

The welcome screen (`apps/desktop/src/renderer/views/AccountWelcomeView.tsx`) offers
**Continue with Apple**, **Continue with Google** and **Continue with GitHub**; **Settings → General → Account**
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
Google and GitHub, with matching headline typography and button sizing. Apple's native
authorization is presented by Clerk. Adding another account lets the user choose
any provider. `AccountSession.signIn(provider: .apple)` calls
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

Every build with Clerk requires sign-in on the phone and the desktop app
([require sign-in](../plans/require-sign-in.md)). Pairing needs a signed-in build
against a cloud that issues device certificates (or a self-hosted cloud, with
`CODYNC_DEVICE_CERT_KEY` on the host); builds without Clerk only keep the
pairings moved over from older versions in the signed-out context, and can't pair.
QR pairing requires the phone and the computer to be in the same account
(phase 2, below). The iPhone's scan step always offers **Skip**,
including when the account has no computers. Skipping finishes onboarding and opens the main
tabs; this choice survives relaunches and account changes. The empty Bots screen
offers **Computers** to pair later. Skipping while adding a computer from settings
only closes that pairing sheet. It does not grant access to a computer or change
the account approval flow. **Start over** resets the onboarding choice.

The desktop app's menu bar **Settings → Reset all data…** (confirmed in the window) is
the computer's start over: it removes this computer from the account, revokes every
paired device, signs out, uninstalls the host, deletes its data folder and the app's
settings, and relaunches into the welcome screen. The host comes back with a new
identity, so phones see the old computer as **No access** (if they were connected),
and phones and the desktop app see it as an older copy once the new one is reachable
(offline, named like a computer they use or see online, and last seen before that one
joined the account) and offer to remove it; pair again or ask for access to reach the new one.

On the computer, open Codync in the menu bar and choose **Pair iPhone…**, then scan the
code on the phone (or paste its `codync://pair` link, or type the short code
`codync-host pair` prints). The phone sends a device certificate from the cloud with
`pair`, and the host checks it offline: a computer in an account lets in phones of that
Clerk user only and turns others away with the owner's masked email (*k•••@gmail.com*);
a computer in no account asks first (desktop dialog, terminal UI, `codync-host pair`,
or `codync-host access approve <code>`), both showing the same 6-digit code, then joins
the phone's account. Apple Hide My Email and Google are different Clerk users, so a phone
on Hide My Email doesn't pair with a computer whose desktop app uses Google: that
refusal is correct, not a regression. Account discovery likewise requires the same
Clerk user ID on both devices.

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
- Sign in with Apple, Google and GitHub test users, verify the avatar/email, restart the app and
  verify session restoration, then log out and verify the anonymous state.
- Cancel the browser flow and verify no error; retry with networking unavailable
  and verify that the application stays usable.

Dashboard creation and the end-to-end Apple/Google/GitHub flows require the user's browser
consent; compilation alone does not verify those steps.

References:
- https://clerk.com/docs/ios/getting-started/quickstart
- https://github.com/clerk/clerk-ios

## iOS account switching

The iOS app now uses the same ClerkKit dependency and shared AccountSession.
Its top-left button opens Accounts. Settings, the tab to the right of State,
holds the phone's own settings. Manage computers opens Computer management,
which holds each computer's settings. The iOS native application must be registered in the same Clerk
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
iPhone (`SharedStore.Context`). Computers paired with a code while signed out
(before sign-in was required) move into the first account used on the phone:
with the device and push keys when that account's context is new on the phone,
otherwise as computers to pair again. Each account context gets its own
`AccountStore`, retired when changing accounts; once Clerk has signed out,
signing out hands the device key back (`forgetDevice` on reachable computers,
`DELETE /v1/devices/{id}` on a token taken before), then erases that account's
computers, device and push keys and caches from the iPhone. **Delete account** deletes the Clerk user first, then does the same.
Signed in, the Computers screen also lists the account's computers from the cloud;
asking one for access shows the 6-digit code to compare on the computer.

Verify with two real Google accounts: add both, switch, cancel OAuth, restart,
check each account's pairings, sign out just one, and verify the other remains.
A simulator build verifies compilation, not the Dashboard settings or OAuth flow.

## Desktop: computers, approvals and SSH

The desktop app attaches this computer's host and its SSH computers over loopback; they
follow into whichever account is active. The account's other computers are not reached
directly (the encrypted channel wasn't ported to the desktop app), so their
"Ask for access" rows show status only. An older copy of a computer the app uses shows
*Older copy · last seen …*. Every offline row has *Remove from account* (`DELETE /v1/computers/{id}`,
confirmed first), so a computer that was reset or given away can be cleaned up even when its
name changed. Offline may only mean asleep, so the dialog says what removal does: account-approved
devices lose access, and the computer's own app doesn't add it back by itself. The phones have no
such action: adding a computer to the account or removing it is desktop-only, and phones manage
only their own access (pair or forget, revoke, ask). See [desktop app](../architecture/desktop-app.md).

**Computers & devices** (Settings) shows, for this computer and each SSH computer:
*Reach from anywhere* (`setCloud`, on/off; the host's build decides which cloud),
*Relay address* and, for a private relay, a write-only *Relay token* (`setCloudUrl`; the
host's `cloudStatus` says `relay: "private"` and `tokenSet`, and there the account rows are hidden
and auto-claim never runs), *Add to account* (claim: `POST /v1/claims` → loopback `claimSign` → complete) and
*Remove from account* (`unclaim`), a pairing QR, and, for this computer only, the computers it can use (its SSH computers, each with its own settings) and the devices that can use it, with revoke.
Signed in, this computer joins the account on its own once per host identity; an identity
that was already in the account and was removed (here or from a phone) is never re-added
silently, only through *Add to account*. Signed in with a different account than the one this
computer is in, the app asks once to move it (`mayOfferMove`): *Move here* leaves the old account
(`unclaim`, which revokes that account's grants) and claims it for this one; *Keep it there* is
remembered for this account (`keptOutOfAccount`). Out of an account, by any path, the host turns
cloud assistants off and the 6-digit check back on (`cloud::pull`). Signing out never removes
the computer from its account. The section below, *Other computers in your account*,
lists only the account's other computers. A device asking for access raises the menu bar dot
and opens the approval sheet with the 6-digit code; Approve stays disabled until the device
revealed its code.

SSH computers use the system OpenSSH (`apps/desktop/src/main/ssh.ts`): `ssh -G` to
resolve the target, `ssh-keygen -F` against `~/.ssh/known_hosts` and
`~/.codync/ssh_known_hosts`, a fingerprint confirmation on first contact (no proxy; the key comes
from `ssh-keyscan`, on Windows from `ssh` itself, since its `ssh-keyscan` can't negotiate with current servers),
`codync-host info --json` for the identity and loopback token, then
`ssh -N -L 127.0.0.1:<free port>:127.0.0.1:<remote port>` with keepalive and backoff.
A changed host key or a different computer ID blocks the connection. Sign-in is key or
ssh-agent only (`BatchMode`: nothing prompts for a password or passphrase); a refused key
stops with a message instead of retrying. The remote command searches Homebrew,
`~/.local/bin` and the Mac app bundle for `codync-host` too, since `sh -l` doesn't read
`~/.zprofile`. At launch the app removes tunnels a crashed or force-quit copy left behind.
Unix uses `pkill` with this app's exact data directory and `ssh_known_hosts` path,
so development sandboxes stay separate. Windows inspects native OpenSSH
processes and terminates only tunnels whose executable and quoted known-hosts
path match this app. It uses Windows' OpenSSH Client feature and the device's
existing SSH config and agent.

A computer without the host shows *Install* on its card (production app only; there is no
released development host): two SSH commands from `main/ssh-command.ts`, the released
host into `~/.local/bin` (`packaging/install.sh --host-only`), then `codync-host install`
on the profile's port. On Linux that step also turns on linger without sudo when it can;
when it can't, the card shows the one `sudo loginctl enable-linger "$USER"` command. Once
the installed computer connects, *Add to account* runs on its own when the app is signed in
and the computer is in no account and not on a private relay (`CloudModel.joinAfterInstall`).

*Over SSH* also offers the `Host` entries of `~/.ssh/config` and the files it `Include`s
(`main/ssh-config.ts`, parser in `shared/ssh-config.ts`): named hosts only, not patterns, `Match`
blocks or git hosting aliases (user `git`), minus the ones already added. The app reads the file
only after the user chose *Allow* in that section; the answer (`sshConfigAccess` in the main
process prefs) is asked once, and after *Not now* a header button turns it on. Add opens the
editor with the alias as Host, so ssh still resolves user, port and keys from the config.

## Verification boundaries

Build with normal simulator signing for Clerk Keychain access. Test Google consent, cancellation, restoration, two-account switching, per-account cache isolation and sign-out on real test accounts. Build success cannot verify Clerk Dashboard state.

For SSH, verify first-contact fingerprints and changed-key rejection. With ProxyJump/ProxyCommand, establish trust in a terminal first. The app checks that the local forwarding listener belongs to its SSH process. The desktop app is not a phone-to-SSH gateway.

Use [Cloudflare testing](cloudflare-testing.md) for remote account approval and relay acceptance.
