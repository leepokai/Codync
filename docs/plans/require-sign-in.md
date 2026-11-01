# Require sign-in

Plan agreed 2026-11-02 for [connections and accounts](../architecture/connections.md): every app is signed in, every phone-to-computer link lives in one account, and QR stays as the in-person way to pair. Phase 1 is built (2.35.0); phases 2 and 3 are not.

## Decisions

- **People sign in; computers join an account.** iPhone, Android and the desktop app require sign-in. The host, the TUI and the CLI never sign in.
- **QR stays.** It is the in-person way to pair, and the only path where the cloud never sees a key. In the last 60 days, 363 people paired by QR and 234 through the account. Codex does the same: it requires sign-in, and the phone still scans a QR, with both devices in the same account.
- **One account per link.** A phone may use a computer only when both are in the same account (phase 2).
- **Headless computers never sign in.** A computer that isn't in an account joins the account of the first signed-in phone that scans it, after the computer confirms (phase 2). There is no `codync-host login`.
- **A managed computer keeps all its own settings.** "Managed computers don't manage others" is a guideline only, not enforced.
- **The desktop app may be signed in to an account other than its computer's.** It is asked once whether to move the computer; after that, one small line in the main window says so.
- **When a host changes the account it belongs to, every device of the old account is dropped**, QR-paired ones included (phase 2). Explain it in those words: the host changed its account, not the app or the phone. Networks are untouched: membership is between hosts, granted by invite, and has its own removal and rules.
- **Builds without Clerk** (no publishable key: source builds, third parties) keep today's signed-out context. It stays only for them and as the migration source; shipping builds never show it.

## Phases

| Phase | Ships | Touches |
| --- | --- | --- |
| 1 | Sign-in required everywhere, signed-out pairings moved into the account, sign-out hands the key back, delete account in the apps, the desktop's account-mismatch line | iPhone, Android, desktop, a small host method, docs |
| 2 | Same-account pairing: account proof on QR, unowned computers join on first scan, refusing other accounts, a host changing its account drops the old account's devices | cloud, host, every client |
| 3 | VPS convenience: one-click host install over SSH, joining the account afterwards on its own, headless hints, short pairing codes | desktop, host, cloud |
| Later | 15-minute lease grace, renames, desktop apps as devices | — |

Each phase is one release with a minor version bump. Phase 1 needs no `minApp` change. Phase 2 does; see below.

## Phase 1: require sign-in

### 1. Delete the account in the apps

App Store 5.1.1(v) and Google Play require in-app deletion for any app that creates accounts. No client has it today.

- **Where:** the account sheet on iPhone (`AccountSwitcherView.swift`) and Android (`AccountSwitcherView.kt`), under **Sign out**; the desktop account menu (`PanelMenus.tsx`).
- **Confirmation dialog**, title "Delete your Codync account?", message: "Your account, its list of computers and every device's access through it are deleted. Your computers keep their bots and chats. This can't be undone." Actions: **Delete account** (destructive), **Cancel**.
- **Steps:**
  1. Delete the user in Clerk first. When that fails ("Couldn't delete your account. Check your connection and try again."), nothing else changes:
     - iOS: `clerk.user.delete()` (ClerkKit `User.delete()`).
     - Android: `User.delete()` from `com.clerk.api.user`.
     - Desktop: FAPI `DELETE /v1/me`, through `apps/desktop/src/main/account.ts`.
  2. Hand the key back to the computers this phone reaches, as signing out does (section 2). The account's stores stay open until then, even though Clerk has already ended the session (`AppStore.leaving` on iOS). The cloud side needs no call: the webhook revokes every device.
  3. Erase this account's local data, the same as signing out.
- **Cloud: no change.** Clerk's `user.deleted` webhook already revokes every grant, cancels pending requests, removes the account's computers from it and deletes their icons (`cloud/src/routes/webhooks.ts`).
- **Clerk setting:** "users can delete themselves" (`user_settings.actions.delete_self`) is on in the development instance. Check it is on in production before the release.
- **Google Play** also wants a web address for deletion requests: add `codync.dev/delete-account`, listing the in-app steps and a contact address.

### 2. Sign-out hands the key back (iPhone, Android)

Today iPhone erases only local data, and Android tells only the computer it is connected to. New order, in `AppStore.signOut()` (iOS) and `AccountStore.signOut()` (Android):

1. **Clerk:** sign out first. When that fails the phone stays signed in and keeps its key, so nothing below runs. The cloud call below needs a session token, so one is taken before (`CloudClient.pinningToken()`); it stays valid until it expires. The account's stores stay open until the steps below finish (`AppStore.leaving` on iOS, `transitions` on Android).
2. **Computers this phone can reach:** call the new host method `forgetDevice`. It removes the caller's own device row and push tickets.
   - Hosts older than this release answer "unknown method"; then call `unregisterDevice`, which stops the push tickets only.
   - The device key is erased right after, so a row left on an older host is unusable.
3. **Cloud:** `DELETE /v1/devices/{id}` for this phone's own device. Find the id the way `CloudClient.revokeDeviceAccess` already does. This revokes every grant and pending request, and hosts drop the device on their next sync. Add `CloudClient.revokeThisDevice()` and its Android twin.
4. Switch context and erase (as today).

The two calls are best effort, with a short timeout each. Signing out never fails because a computer or the cloud is unreachable. **Start over** hands the open account's key back the same way before it erases everything.

**Host (`host/src/api/remote.rs`):** `forgetDevice` is for a device caller only. It removes `caller.device_key()` from devices and push tickets. It sends no event: a desktop showing the device list sees the change the next time the list loads. It is additive, so `minApp` doesn't change.

### 3. Sign-in gate

| Clerk | State | Shown |
| --- | --- | --- |
| Configured | Restoring (`isReady == false`) | The launch splash. The last active context stays loaded, as `AppStore.init` already does, so an offline launch keeps working |
| Configured | Ready, signed out | The sign-in screen |
| Configured | Signed in | Today's flow: pairing when there's no computer, else the tabs |
| Not configured | — | Today's flow, signed-out context |

**iPhone (`RootView.swift`, `WelcomeView.swift`)**
- `WelcomeView` keeps the crew, the chat glimpse and **Watch the demo**.
- With Clerk configured, **Get started** is replaced by `SignInButtons` (Apple, Google) as the primary actions.
- The pairing screen's **Skip** stays: a signed-in user may look around before pairing.
- When the signed-out context has computers, one more line appears: "Sign in to keep using your {n} computers. They move into your account; you won't need to scan again."
- `RootView`'s condition becomes "configured and ready and signed out" instead of `!onboardingCompleted && !welcomed`. Signing out of the last account lands back on this screen.
- `AccountSession.signIn` without Clerk is unreachable once the buttons are hidden. Its message "You can continue using local pairing" goes.

**Android (`WelcomeView.kt`, the root in `CodyncApp.kt`)**
- The same screen and the same rules.
- `NativeAccounts.configured` stands in for `isConfigured`.

**Desktop (`ChatWindow.tsx`, `AccountWelcomeView.tsx`)**
- With Clerk configured, **Continue on this Mac** (the button and the skip link) goes, and the welcome shows only **Continue with Apple** / **Continue with Google**.
- `onboarded` alone no longer opens the chat window. The welcome shows whenever the app is configured, ready and signed out, so signing out from the account menu returns to it.
- The host keeps running while signed out, and so do the tray and menu bar.
- Signing out also revokes the desktop app's own cloud device (`DELETE /v1/devices/{id}`, best effort, on a token taken before Clerk signs out) once Clerk has signed out, then its key file goes: the cloud refuses a revoked key, so the next sign-in makes a new one.
- A saved session stays "restoring" (retried every 30 seconds) until Clerk turns it down (401, 403 or 404) or answers, so a launch offline, behind a captive portal or during a Clerk outage isn't stopped at the welcome.
- Builds without Clerk keep **Continue on this Mac**.

### 4. Move signed-out pairings into the account

Runs on iPhone and Android when an account becomes active and the signed-out context holds computers or a device key. It runs before that account's stores and identity load: `AppStore.switchAccount` and `AppStore.init` on iOS, `loadContext` on Android.

| This phone's account context | What happens |
| --- | --- |
| **New** (no device key, no computers) | Everything moves: the device and push keys, the computers, order, drafts, shortcut lead, widget snapshot. Hosts see the same key, so nothing is paired again and push keeps working |
| **Already used** (has its own key) | The computer lists merge (the account's entry wins for the same id). Computers paired only with the signed-out key are refused by their hosts; they show the existing "pair again" state (`isStale`) |

After either case the signed-out context is erased, and when computers moved an analytics event `signed_out_pairings_moved {computers, keys: moved|rescan}` is sent (in [analytics events](../reference/analytics-events.md) and both apps).

**iOS**
- `SharedStore.Context.adopt(from:)` in the Kit:
  - Moves the Keychain items `com.pokai.Codync.device-key` / `push-key` from account `local` to the context id.
  - Moves the `UserDefaults` names in `Context.names` to their `account.<id>.` keys.
  - Drops `ActivityIconCache` and the `codync-mirror-local-*` caches, which rebuild.
  - The Kit's test bundle has no Keychain entitlement, so its tests keep keys in memory (`Keychain.memory`, debug builds only).
- Unit tests in `CodyncKitTests` cover both cases.

**Android**
- `LocalStore.adopt(from)`:
  - The identity is sealed with a per-scope Keystore alias and AAD, so the seeds are decrypted with the old key and sealed again under the new scope.
  - Every other file in the folder is copied.
  - In the merge case only `computers.json` merges.
- Unit tests cover both cases.

### 5. Desktop: computer in another account

- **Logic:** a pure function next to `mayOfferMove` in `lib/auto-claim.ts`, `accountMismatch(host, userId, keptOut, seen)`. It returns one of three values:
  - `other {email}`;
  - `none {reason: cloudOff | privateRelay | keptOut}`;
  - `null` when the accounts match.
  Tested in `auto-claim.test.ts`.
- **A notice** above the sidebar footer's buttons (`SidebarFooter.tsx`): an amber card with an icon that shows its text in full, wrapping as needed, and takes no space when there's no mismatch. The narrow rail shows it as an amber icon with the text as its tooltip.
  - `other`: "This computer is in {email}'s account", with the full address as Settings and the move dialog show it. Clicking it reopens the move choice (`MoveComputerPrompt`), even after *Keep it there*.
  - `none`: "This computer isn't in an account: its cloud is off" / "…: it uses a private relay", or "This computer isn't in your account" (taken out). Clicking it opens Settings → Computers.
- **The move dialog** stays as it is in this phase. Its "QR-paired devices keep access" text changes in phase 2.

### 6. Docs and release

- **User manual**
  - `getting-started.mdx:73` and `remote-access.mdx:31`: drop "No account is needed".
  - `linux-servers.mdx`, "Pair your phone": the phone must be signed in.
  - Add account deletion to the account page.
- **Privacy policy** (`web/app/privacy/page.tsx:40`): "Codync works without an account" no longer holds. The maintainer reviews the new wording.
- **Repo docs**
  - `connections.md`: the work table.
  - `remote-relay.md` §3: the phone's contexts.
  - `analytics.md`: the new event.
- **WhatsNew** (iOS and Android, zh-Hant and en-US): sign-in is now required; your computers come with you; deleting your account.
- **App Review notes**: reviewers sign in with Sign in with Apple. The account features are remote access requests, computers across phones, cloud assistants and screen relay (answers 5.1.1(v): why sign-in is needed). The existing pairing demo video stays.
- **Version:** minor bump (2.35.0 from 2.34.2) in every place [versioning](../../CLAUDE.md) lists. The host changes too, because of `forgetDevice`.

### Tests and end-to-end

**Unit tests:** the Kit (`adopt`, both cases), Android (`LocalStore.adopt`), desktop (gate and `accountMismatch`), host (`forgetDevice` removes only the caller).

**End-to-end** on the iOS Simulator, the Android emulator and the desktop `npm run dev` sandbox, with the dev Clerk test accounts (`npm run phone-account`):

| Check | Expected |
| --- | --- |
| Fresh install | The welcome shows only sign-in. Signing in leads to pairing, and a QR pair works |
| Upgrade | With the previous build signed out, pair by QR. Install this build: the gate counts the computer. Sign in: the computer is there and connects without scanning, and push still arrives |
| Merge case | Unit tests only: with sign-in required, a phone can't pair signed out any more, so this only happens to someone who was signed in on an older build and had also paired signed out (the older builds can't take the test sign-in ticket) |
| Sign out | The computer's device list drops the phone, and the cloud lists the device as revoked |
| Delete account | The test user is gone in Clerk, and the computers leave the account (`--prune` cleans up) |
| Desktop | Signed out shows the welcome with no skip. Sign out returns to it. Signed in to a second test account, the footer notice shows and reopens the move choice |

### Risks

- **Signed-out regulars may leave.** That is 84 iOS and 9 Android users active on 3 or more days in 30 days. Watch the gate's sign-in rate (`sign_in_started` → `signed_in` from the gate screen) for two weeks after the release.
- **A Clerk outage** stops new installs at the gate. Existing sessions load from the Keychain and keep working.
- **App Review** may question requiring sign-in when QR alone works. The review note above answers it.

## Phase 2: same-account pairing

**Account proof**
- The cloud signs a short-lived device certificate `{deviceKey, userId, exp}` with its own Ed25519 key (`POST /v1/devices/certificate`). It publishes the public key at `/v1/keys`.
- A host pins that key when it joins an account and keeps it with the owner. Checking a certificate is a signature check, so it works offline, with the cloud off or on a private relay.

**`pair` takes the certificate**

| The computer | Result |
| --- | --- |
| Has an owner | The certificate's `userId` must equal the owner. Otherwise the error is `otherAccount`, with the owner's masked email |
| Has no owner | The computer confirms first. The desktop shows a dialog: a new `pairingRequest` event and a `decidePairing` method. `codync-host pair` and the TUI ask `y/N` with a matching code. After that the host joins the phone's account through a new host-signed route that takes the certificate, and pins the cloud key |

**Each device records its account.** Devices get `account_user_id`. When the host changes the account it belongs to (joining one, moving, or leaving with **Remove from account**), devices of any other account are dropped. The move dialog's text then reads: "Every device of {email}'s account loses access." Networks stay as they are: a computer handed to someone else is taken out of a network by the network's creator or an admin, like its SSH key is taken off the manager.

**Phones:** send the certificate when pairing, and explain `otherAccount`: "This computer is in a•••@gmail.com's account. Move it to your account in Codync on that computer, or sign in with that account."

**Compatibility:** apps older than this release can't send a certificate. A host refuses their QR pairing with `unsupportedVersion` ("Update Codync"). That is a new requirement on the phone, so `minApp` rises with this release and [the release gate](../reference/compatibility.md) holds it until both stores have the apps.

## Phase 3: VPS convenience

- **One-click install over SSH** (`apps/desktop/src/main/ssh*.ts`):
  - On `notInstalled`, **Install** runs `install.sh --host-only` and `codync-host install` over the existing SSH connection, with a new `installing` SSH state.
  - `loginctl enable-linger` needs sudo. When it fails, the card shows that one command to run. It replaces today's `brew` command, which fails on Linux.
- **Joining the account after install:** the manager app's **Add to account** runs on its own, because installing shows the intent.
- **Headless hints:** `codync-host cloud` and the TUI account screen say which account the computer is in, or how to add it ([connections: headless](../architecture/connections.md#headless-computers)).
- **Short pairing codes:** `codync-host pair` also prints a code like `QRTX-4821`, which the host registers with the cloud as a pairing offer. The app resolves the code to the pairing link. The terminal's matching code catches a cloud that swaps keys.

## Later

- Honor the 15-minute lease when the relay drops; today account devices are refused until the cloud state syncs again.
- Renames: `local`, `owner`, the desktop's "{name} can use".
- Desktop apps as devices (the encrypted channel on the desktop), so one computer can use another's bots.
