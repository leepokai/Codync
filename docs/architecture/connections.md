# Connections and accounts

Who may reach a computer, how, and what each state is called. Agreed 2026-11-02 as the target design; [Work](#work) lists what is built and what isn't yet. Wire details stay in [remote relay](../reference/remote-relay.md).

**People sign in; computers join an account.** An app someone uses (iPhone, Android, desktop) signs in. A host never signs in: it is *in* an account, which the cloud records as the computer's owner. The computer keeps no account credentials, so a compromised server gives up its own bots, never the account.

## Terms

| Term | Meaning | Code today |
| --- | --- | --- |
| Account | One person's Codync sign-in (Clerk user) | `userId` |
| App | iPhone, Android or desktop app. Must be signed in | — |
| Computer | One `codync-host`, known by its own key. In at most one account | `computerId`, `signKey` |
| Device | An app instance allowed to use a computer, with its own key | `Device`, `deviceKey` |
| In an account | The account a host belongs to (its owner in the cloud). Joining is a claim the host signs. "The host changed its account" means joining, moving or leaving, never an app signing in to another account | `claim`, `owner_user_id` |
| Manager / managed computer | A desktop app that manages another computer's host over SSH, and that computer | SSH profiles (`ssh-profiles.json`) |
| In person / remote | The two ways a device gets to use a computer: QR (or short code) at the computer, or the account list plus a 6-digit check | `DeviceSource::Local` / `Account` |
| Route | Direct (LAN, Tailscale), Codync relay, private relay, SSH tunnel. A path, not a relationship | `HostRoute` |

The terminal UI and the CLI are the computer's own console, not apps in this sense: they never sign in.

## Relationships

| Relationship | From | To | Granted by | Direction |
| --- | --- | --- | --- | --- |
| **Manage**: settings, approvals, installing | App (on the computer, or over SSH) | Host | The OS account (SSH keys) | One way |
| **Use**: bots, chat, screen | Device | Host | The computer, in person or remote | Device → computer |
| **Network**: bot to bot messages | Host | Host | codync-network invite | Both ways |
| **Sign in** | App | Account | The identity provider | — |
| **In an account** | Host | Account | A claim the host signs | — |

- The account is not a link. It is an address book (find your computers without a QR), a remote doorbell (ask for access, revoke a lost phone) and cloud features (screen relay, custom icons, cloud assistants).
- Manage is between a desktop **app** and a **host**, not between hosts: the SSH list lives in the app's data folder and the app opens the tunnel itself. The manager's own host takes no part, and the managed host doesn't know it is managed. Codync and Codync Dev on one Mac keep separate lists.
- Network is the only host-to-host relationship. A manager adds a managed computer to its network by calling that host's network methods through the SSH tunnel.

## Rules

1. **The computer decides.** The cloud can only shorten access, never grant it.
2. **Every app is signed in.** No signed-out mode remains, on any phone or desktop build that ships.
3. **One account per link.** A device may use a computer only when both are in the same account.
4. **A computer is in at most one account.** Changing accounts means leaving one and joining the other.
5. **Manage comes from the OS account.** Whoever sits at a computer can read `~/.codync/token`; Codync doesn't pretend to stop them. A managed computer keeps every setting of its own.
6. **Signing out affects only that app.** It hands its key back (cloud grant revoked, push stopped, the device removed on computers it can reach) and erases that account's data on the device. The computer stays in its account; leaving takes **Remove from account**.

## Pairing a phone

The phone finds and pairs every computer itself. A computer the phone hasn't paired shows in its account list (greyed, **Ask for access**), never with bots.

| Where you are | How | The computer |
| --- | --- | --- |
| At the computer | Scan the QR it shows (`codync-host pair`, the desktop's **Pair iPhone**, an SSH computer's card), or type its short code when scanning a terminal is awkward | In your account: the phone gets in. In no account: asks once (desktop dialog, terminal `y/N` with a matching code), then joins the phone's account and lets the phone in. In another account: refuses, "This computer is in another account" |
| Away | Tap it in the account list, compare 6 digits on the computer, Allow (or **Skip the 6-digit check**) | Must already be in your account |

- Only a signed-in phone scans, and the host checks the phone's account from a proof the cloud signed, never from what the phone says. The check works offline (a signature), so computers with the cloud off or on a private relay still pair in person.
- A QR pairing records the phone's account on the computer. When a host changes the account it belongs to, it drops every device of the old account, QR-paired ones too.
- Joining an account on the first scan is how a headless computer "signs in": there is no `codync-host login`. A computer may also join through a manager's **Add to account**, which uses the manager app's account.

## When the app and its computer disagree

The desktop app runs on a computer with its own host, so its account and the computer's can differ.

| Computer | Its own app | Shown |
| --- | --- | --- |
| In A | Signed in to A | Nothing |
| In A | Signed in to B | Asked once, **Move this computer to your account?** (*Keep it there* is remembered). After that, an amber notice above the sidebar's account button, its text in full: "This computer is in a•••@gmail.com's account", which opens the same choice |
| In no account | Signed in | Joins on its own (`mayAutoClaim`), unless the cloud is off, it uses a private relay, or the user took it out; then the same notice says which, and opens Settings → Computers |

The app manages its computer through loopback either way. Only same-account phones can pair with it (rule 3), so a B phone that tries is told which account the computer is in and how to move it.

## Headless computers

A computer with only the host (SSH-only server, VPS) never signs in. It joins an account in one of two ways:

- A manager's desktop app adds it over SSH: add the SSH computer, install the host in one click, then it joins the manager app's account on its own.
- A signed-in phone scans `codync-host pair` (or types its short code) and the terminal confirms.

`codync-host cloud` and the TUI's account screen say which account the computer is in, or how to add it to one.

### Using a Linux VPS

The agents themselves (Claude Code, Codex, …) always sign in over SSH, as the user that runs the host: `claude`, `codex login --device-auth`. Codync can't do that for them. Everything else depends on whether there's a desktop app to manage the VPS.

**With the desktop app (recommended)**

| Step | Where | What | Today |
| --- | --- | --- | --- |
| 1 | VPS | Install the agents and sign them in | Same |
| 2 | Desktop | Settings → Computers → Over SSH → **Add SSH computer**, pick the host from `~/.ssh/config`, compare the host key, **Trust and connect** | Built |
| 3 | Desktop | **Install** on the card: `install.sh --host-only` and `codync-host install` over SSH. Linger is turned on where the system lets the user do it (Ubuntu 24.04 does); otherwise the card shows `sudo loginctl enable-linger "$USER"` | Built (2.36.0); Codync Dev shows the command instead |
| 4 | Desktop | The VPS joins the desktop app's account on its own | Built (2.36.0) |
| 5 | Phone | The VPS appears in the account list: **Ask for access**, then compare the 6 digits on the desktop and Allow. Or open **Pair iPhone** on the VPS card and scan | Built |
| 6 | Desktop | Optionally add it to the network in one click | Built |

**Phone only**

| Step | Where | What | Today |
| --- | --- | --- | --- |
| 1 | VPS | Install the agents and sign them in | Same |
| 2 | VPS | `curl -fsSL …/install.sh \| sh -s -- --host-only`, `codync-host install`, `sudo loginctl enable-linger "$USER"` | Built |
| 3 | VPS | `codync-host pair` prints a QR code and a short code | Built (2.36.0) |
| 4 | Phone (signed in) | Scan, or type the code in the paste field. The terminal asks to add the VPS to this account and let this phone in, with a matching code; answer `y` | Built (2.36.0) |
| 5 | Later | Other phones of the same account scan once and are in. Phones that ask remotely are approved with `codync-host access approve` over SSH, or by turning on **Skip the 6-digit check** (`codync-host cloud approval auto`) | Built (2.36.0) |

Either way the VPS uses the Codync relay by default, with no port to open; Tailscale on both ends gives a direct route. A headless VPS has no remote screen, and its new bots start without connectors (no credential store). `codync-host tui` over SSH chats on the VPS itself.

## States

| Belongs to | States | Code today |
| --- | --- | --- |
| Computer's account | Not in an account · In your account · In another account; also: Cloud off | `cloud.owner`, `cloud.enabled` |
| App vs its computer | Same · Computer in another account · Computer in no account | `accountMismatch` (desktop) |
| Manage (SSH) | Idle → Connecting → Check host key → Not installed → Installing → Connected; on errors: Retrying · Failed | `SSHStatus` |
| Use: access (device × computer) | None → Waiting for OK → Can use (in person / remote); later: Can't confirm right now · Removed | `unpaired`, `pending`, `local` / `account`, `leaseExpired`, `revoked` |
| Use: connection | Connecting · Online · Computer offline · Offline | `Connection` |
| Network role | Not in a network · Member · Admin · Creator | `Role` (`owner` becomes Creator) |

Words retired with this design:

- **Signed-out mode**, the phone's local context.
- **`owner`** in three senses: a computer's account is "in your account", a network's first member is the Creator, and the manager is the manager.
- **`local`** in four senses: loopback stays loopback, QR-paired devices are "in person", the signed-out context goes, the desktop's own computer is "this computer".
- **"{name} can use"** in desktop settings, which lists computers managed over SSH; it becomes Manage.
- **master / slave** never appear in the UI; a managed computer may show "Managed by {name}".

## Not doing

- **Phones over SSH.** SSH grants Manage; a phone needs Use, which the encrypted channel already gives, revocable per device. Tailscale is the direct route.
- **Reverse SSH from a managed computer.** It would make the managed computer a manager of its manager. A managed computer that wants another computer's bots asks to use it, like a phone.
- **Locking a managed computer's settings.** It can't hold against the CLI, and the managed computer stays its own.
- **Managed computers managing others.** A guideline, not enforced: nothing records "I am managed", chains and cycles add no power, and the manager list lives in the app.

## Work

Phases, file-level steps, tests and risks: [require sign-in](../plans/require-sign-in.md).

| Order | Change | Status |
| --- | --- | --- |
| 1 | Delete the account from inside the apps (App Store 5.1.1(v), Google Play). The cloud already cleans up on Clerk's `user.deleted` | Built (2.35.0) |
| 2 | Sign-out hands the key back (iPhone, Android; the desktop revokes its cloud device) | Built (2.35.0) |
| 3 | Sign-in required in every app; the phone's signed-out pairings move into the account on first sign-in (with their device and push keys when the account context is new on that phone, otherwise a list to scan again). Builds without Clerk keep the signed-out context | Built (2.35.0) |
| 4 | Same-account QR: the cloud-signed account proof, the host recording each device's account, joining an unowned computer on first scan with a confirmation, short codes, the refusal message | Built (2.36.0) |
| 5 | Changing accounts drops every old-account device; the move dialog's text says so | Built (2.36.0); joining a first account keeps the phones paired before 2.36.0 |
| 6 | The main-window notice for an app and computer in different accounts; the phone's refusal text | Built (2.35.0, 2.36.0) |
| 7 | One-click host install over SSH, then **Add to account** on its own | Built (2.36.0) |
| 8 | Honor the 15-minute lease when the relay drops | Not built: account devices are refused until the cloud state syncs again |
| 9 | The renames above, in every client | Not built |
| 10 | Desktop apps as devices (encrypted channel), so one computer can use another's bots | Not built |

## Prior art (checked 2026-11-02)

- **Codex**: remote control needs a ChatGPT sign-in, and the phone still scans a QR from the computer; both must be in the same account and workspace, and the computer approves remote access before it shows the QR ([remote connections](https://learn.chatgpt.com/docs/remote-connections)). Headless machines sign in with `codex login --device-auth` and pair with a short code from `codex remote-control pair`.
- **OpenClaw**: no account. QR or setup codes first, "because that is what most people were trying to find anyway"; new remote devices wait for `openclaw devices approve`, and SSH-tunneled connections are trusted as local ([pairing](https://docs.openclaw.ai/gateway/pairing)).
- **Hermes Agent**: no official phone app; chat users pair with an 8-character code the owner approves; anyone who signs in to the web dashboard acts as the owner ([docs](https://hermes-agent.nousresearch.com/docs/)).
