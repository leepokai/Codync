# Optional sign-in

Agreed 2026-11-06, built in 2.41.0. Reverses part of [require sign-in](require-sign-in.md): apps may stay signed out and a computer may stay out of any account. [Connections and accounts](../architecture/connections.md) has the result; this page has the decisions and the steps left.

## Decisions

- **The Codync relay needs an account on both ends:** the computer in an account and the phone signed in to that same account. A computer in no account is direct only, whoever the phone is.
- **A signed-out phone may pair only with a computer in no account.** A computer in an account refuses it, or leaving out the certificate would get around the same-account check.
- **A signed-in phone scanning a computer in no account gets asked once:** "Add this computer to your account? Without it, this phone can connect only over Wi-Fi or Tailscale."
- **A computer in no account may have phones of different accounts.** It confirms each one.
- **TURN stays relay only.** `hostScreenIce` already requires an account and doesn't change. A direct session contacts no STUN or TURN provider ([remote screen](../features/remote-screen.md)).
- **Public webhooks need a computer in an account.**
- **A computer that joins an account removes its signed-out phones and other accounts' phones.** A dialog warns first and names them; Add to account waits for the names, and a computer that can't say gets a general warning. They pair again, signed in to that account.
- **The screen follows the Connection choice:** Auto goes direct, then TURN; Cloudflare first the other way round; Wi-Fi or Tailscale only never asks. A computer in no account shows no choice.

| Phone | Computer in no account | In the phone's account | In another account |
| --- | --- | --- | --- |
| Signed out | Direct only | Refused | Refused |
| Signed in | Direct only, until the computer joins | Direct and relay | Refused |

## Defaults taken

These follow the recommendations already given. Say if one is wrong.

- Desktop sign-in and SSH install ask the same question instead of joining on their own.
- Push stays open to signed-out phones. `relay/` has no account concept; the phone opens to the unreachable state when it can't connect.
- Direct means anything off Codync's infrastructure: LAN, Tailscale, `CODYNC_EXTERNAL_URL`, the user's own tunnel, a private relay.
- Signing out returns to the welcome screen. The account's computers and chats stay locked, and the signed-out space is separate and starts empty.
- LAN discovery ships in the first version, because any phone of a computer in no account has no other way to find a changed address. It advertises only while the computer is in no account, so a computer in an account never needs a multicast permission.
- A message that can't leave a phone with no relay waits there and goes when the computer is back, with the same nonce, also if the computer has joined an account since.

## How it's built

- **Cloud:** `relayDevice` checks `mayUseRelay`: outside pairing, the computer needs an owner and the device key a row under that owner, not revoked and not signed out; a pairing socket needs a registered key. `POST /v1/hooks/...` needs an owner. Both sit behind `RELAY_NEEDS_ACCOUNT` (below).
- **Host:** a missing certificate is a signed-out phone (`channel/pairing.rs`); an invalid one is refused. `pairing_requests` answers `Join | PhoneOnly | Decline`. `devices.no_account` tells signed-out phones from rows before 2.36.0. `hello` sends `cloud` only where a phone may use it and `canJoinAccount` for the offer; `joinAccount` asks the computer. `relay::run` holds no socket in no account except for a live pairing code or a claim just signed, and pulls state every 5 minutes without one. `remote/discovery.rs` advertises `_codync._tcp` with the computer's addresses in TXT.
- **Phones:** **Continue without an account** on the welcome screen opens the `local` context, which `adopt` moves into an account at sign-in: its pairings and messages, and its key only when the account is new on this phone. Otherwise the computers ask to be scanned again. **Add to account** on the computer's row. `ScreenRoute` picks the screen's way. `LocalDiscovery` feeds found addresses to the direct race.
- **Desktop and terminal:** `JoinAccountPrompt` asks once for this computer and for each **Add to account**. `PairingRequestPrompt`, the terminal UI (`y`/`w`/`n`), `codync-host pair` and `codync-host access approve --phone-only` take the new answers.

## Compatibility and rollout

Additive, so `minApp` stays. `hello.joinAccount` stays, always `false`, so older apps never call `joinAccount`, which takes a certificate now. Older apps on a computer in no account lose the relay and have no **Add to account** row ([compatibility](../reference/compatibility.md)). A signed-out phone against an older host gets `UnsupportedVersion` and shows "update the computer". Older signed-in apps always send a claim; on `PhoneOnly` the host ignores it, and those apps already handle pairing without one.

Left to do, in order:

1. Ship 2.41.0. `phone_connected.route` starts counting how many people depend on the relay.
2. `npm run deploy:dev` with the release PR, and test on dev: `RELAY_NEEDS_ACCOUNT` is already on there in `wrangler.toml`.
3. Once both stores have the apps, count computers with no owner that still use the relay (D1 and the DOs) and tell those users how to join. Hosts before 2.41.0 still hold a socket with no owner. The message also names public webhooks: a 2.41.0 host in no account holds no socket, so until step 4 the cloud accepts a delivery to an old public URL with `202` and nothing picks it up; it expires after 72 hours.
4. Set `RELAY_NEEDS_ACCOUNT = "1"` in main's `[vars]`, then `npm run deploy:main`. Later, remove the switch.
