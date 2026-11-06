# Connector setup and credentials

Codync uses one host-owned connection workflow on iOS and the desktop app (macOS, Linux). A
Installing from Extensions and a bot's connection request use the same installation,
OAuth and verification endpoints. The clients never implement a separate password
store. A phone submits credentials through its authenticated encrypted channel to
its selected host.

## Storage

`market/vault.rs` encrypts sensitive records with XChaCha20-Poly1305, a random
24-byte nonce and the record key as authenticated associated data. A random
256-bit key belongs to this host database's random vault ID. It is held by the OS:

- macOS: Keychain, via the native keyring backend.
- Linux: freedesktop Secret Service over the user's session D-Bus, with encrypted
  transport and a vendored D-Bus client. GNOME Keyring or another compatible
  unlocked Secret Service is required. Run the host in that user's session.
- Headless Linux: provide a session D-Bus and unlocked Secret Service for the
  service user. An unavailable keyring gives an actionable error. There is no
  plaintext, kernel-keyring-only or automatic ephemeral fallback.

Unlocking runs on a blocking worker. The key is cached in zeroizing memory until
host exit. Locking the desktop keyring after unlock does not evict that cache;
stop the host to remove it. The implementation does not claim to isolate secrets
from arbitrary code running as the same OS user.

Encrypted records include bot secrets, connector environment, headers, command arguments and
OAuth tokens, the Composio config, agent API-key environment, and the optional
1Password service-account token. Existing records migrate on their first read
with an available key. Missing keys and corrupt ciphertext are errors, never an
empty replacement configuration. SQLite secure deletion is enabled; historical
backups and preexisting WAL/filesystem snapshots can still contain older data.
Back up the OS credential store together with the database. A database copied to
another machine without its key cannot decrypt credentials. There is no automatic
cross-host or cloud-account credential sharing.

## Execution boundary

ACP receives local connector proxy commands with an empty environment, rather
than credential values. The proxy gets its configuration through authenticated,
loopback-only `connectorRuntime` and injects values into the connector process.
Remote connectors use the existing host OAuth proxy. General listings omit secret
values and command arguments. The local API token and same-user host access are
trusted capabilities; this is not a sandbox against the agent's own shell tools.

A readiness check starts the connector, performs MCP initialize and tools/list,
and stops the check process. It never invokes a business tool. Failures and a
90-second timeout remain visible and retryable. Package acquisition is performed
by the configured runtime (npx, uvx, Docker, etc.), so that runtime must be present.

Installing a registry connector that is already installed (Add again after a failed
check, or Connect on a request card) keeps it as it is when nothing new is typed and
it would run the same way, so its sign-in survives. New values or another way to run
it (package runtime or hosted) rebuild it in place, under the same id, so each bot
keeps it on or off as before (`install_server` in `host/src/market/install.rs`).

## Conversation requests

The built-in `connectors` MCP offers search_connectors, list_connectors,
request_connection, request_app (Composio), and request_secret. Requests become
notice entries with structured connectionRequest data. They identify the requesting
bot, service, destination field, purpose and status; they contain no credentials.

Clients submit values through connectorRequestFinish. The host validates the
request, destination, requesting bot and 24-hour expiry, stores credentials,
verifies availability, enables the connector for that bot, and queues a non-secret
acknowledgement in the original conversation. Repeated completed submissions
return the completed entry. Verification failures remain pending so the user can
correct credentials and retry. Current clients offer no way to cancel a request
(it stays until it's done); `cancel: true` remains for older clients, and a
cancelled request rejects submissions.

The host posts one more kind itself, `agent` (`market/agent_sign_in.rs`): when a bot's
own agent (Claude Code…) refuses a turn because it needs signing in, the chat gets a
card naming the agent (`backend`) and the user messages it couldn't answer
(`entryIds`) instead of an error. Only agents Codync knows by name get the card;
others show the agent's error. A new bot gets the card before its first message
when the agent's sign-in check, run in the background as the bot is created, finds
it signed out. When a [fallback agent](agent-fallback.md) takes the turn over, or a bot skips
an agent another bot found signed out, the card comes anyway, holding no message (the fallback
answers it). A bot keeps one such card in each chat; messages that fail later join it. The
card's Sign in opens the agent's sign-in sheet from Extensions. As soon as a sign-in
check (`agentAuth`, `agentAuthenticate`, `setAgentEnv`, connectorRequestFinish on the
card, a new bot's check, or the re-check at host start and `refreshBackends`) finds
the agent signed in, or a turn of the agent finishes (signed in outside Codync),
every pending card for it completes, its messages go out again and bots that moved on to a
fallback while it was signed out come back on their next message;
a message older than 24 hours isn't sent again. The card itself doesn't expire.

MCP connection requests and runtime secret retrieval are loopback-only. Authorized
Control devices may submit or cancel requests. No endpoint exposes a generic
credential getter to a device. Existing local-process trust boundaries still apply.

## Website and app logins

`market/logins.rs` keeps sign-ins (`site` = domain or app name, username, password
or `op://` reference) as one vault record. Bots use `list_logins` (site and
username only) and `request_login {site, reason}`, which becomes a login card; the
user saves username and password there (or in Credentials → Sign-ins), and the bot
is resumed with the login's id. The `computer` tool `type_login {login, field}`
types it into the focused field through the screen helper and returns only a
screenshot. Before typing, the helper's `focusedField` reports the front app, the
page URL (macOS AX `AXWebArea`/`AXURL`, Linux AT-SPI `DocURL`) and whether the field
is a password field. The host refuses unless the page host is the site or a
subdomain (or, without a page, the app name equals the site), and types a password
only into a password field. Clients manage logins with `credentialLogins`,
`credentialSaveLogin` and `credentialRemoveLogin`; no endpoint returns a password.

Terminal password prompts (sudo, ssh) are not covered: they aren't password fields
to the accessibility API. The same-user boundary above applies.

## Bot secrets

`market/bot_secrets.rs` keeps secrets a bot needs for its own work, one vault record
(`botSecrets`) for all bots: an API key pinned to one HTTPS origin, or a command
variable. `market/secret_tools.rs` gives the bot these `connectors` MCP tools:

| Tool | Does |
| --- | --- |
| `list_secrets` | The bot's secrets: name, destination, never the value |
| `request_api_key {name, origin, auth, header?, username?, reason}` | A secure card for a key on one public HTTPS origin; `auth` is `bearer`, `header` (a header the bot names) or `basic` (username + the secret as password) |
| `request_command_variable {name, reason}` | A secure card for a variable a shell command needs |
| `api_request {secret, url, method?, headers?, body?}` | The host sends the request with the key attached. The URL must be on the key's origin, without a `user:password@`, and pass the [egress policy](../reference/egress-policy.md) (public HTTPS, same-origin redirects only, 30 s, 1 MB answer). The bot can't set `Authorization`, `Proxy-Authorization`, `Cookie`, `Host` or the key's own header, and sends at most 30 headers. |
| `run_with_secrets {command}` | Runs the command in the bot's folder with every command variable in its environment, named in upper case with `-` as `_` (`netbird-setup-key` → `$NETBIRD_SETUP_KEY`, `%NETBIRD_SETUP_KEY%` on Windows). The command gets its own process tree (Unix process group, Windows job), killed when it ends or after 60 s, so nothing it started keeps the variables. The answer keeps the last 64 KB of each stream. |

Names are letters, digits, `-` and `_`, starting with a letter; a name that would
become a variable the shell relies on (`PATH`, `HOME`, `IFS`, `ENV`, `BASH_ENV`,
`SHELL`, `USER`, `COMSPEC`, `PATHEXT`, `SYSTEMROOT`, `TEMP`, `TMP`, `TMPDIR`,
`USERPROFILE`, `LD_*`, `DYLD_*`, `CODYNC_*`) is refused. On Windows `cmd` expands a
variable only once when a line is handed to it through another variable, so the
host writes the line to a temporary batch file. To keep the line meaning what it
would mean typed, the host doubles every `%` that isn't part of `%NAME%` (or
`%NAME:…%`) for a defined variable.

The card is the existing `secret` connection request, so every client shows it
without changes: `title` is the service's host (or the variable), `field` the name,
`location` the destination the host wrote and `botSecret` where the value goes.
On `connectorRequestFinish` the host saves the value with the destination from the
card, never one from the client, and resumes the bot with a note naming the tool to
use. Asking again for a name the bot already has replaces its value. A value is
trimmed and must be 3 B to 16 KB (an `op://` reference is resolved on each use). A
bot keeps at most 100. Deleting a bot forgets its secrets, unless the vault is
locked at that moment; then Credentials lists them under "A removed bot" so you can
remove them there.

Values are read fresh for each use. In what comes back (answer bodies, command
output), the host replaces each value with `[secret]`. It also replaces the
value's JSON-escaped form and, for basic auth, its base64 form, before cutting the
output to length. Values never reach the agent's environment, the transcript or
the log: `api_request` and `run_with_secrets` log only the bot, the secret name,
the host and the status. This is not a sandbox: a command the bot wrote can still
encode and print the value it was given.

Credentials → **Bot secrets** (desktop, iPhone, Android; hosts 2.20.0 and newer)
lists them through `credentialBotSecrets` (`{id, botId, botName, name, use,
destination}`) and removes one with `credentialRemoveBotSecret {id}`; the next use
fails. The terminal UI completes the card but has no Credentials screen, as with
sign-ins.

Idea from rakazo's `docs/bot-secrets.md` (Apache-2.0); see
[upstream attribution](../reference/upstream-attribution.md).

## Optional 1Password

Credentials settings accepts a service-account token and checks it using the
installed `op` CLI. Use a dedicated vault and grant the service account access only
to that vault. Codync does not create a 1Password account or vault automatically.

Use an `op://vault/item/field` reference as an environment value or complete header
value. The host resolves it when verifying/launching a local connector or preparing
a remote request. The token goes in the child environment, not command arguments;
output is consumed by the host and errors do not echo provider secret values.
Disconnecting removes the active token; connectors using references then require
reconnection. OAuth remains the preferred flow for services that support it.

## Validation

Host tests cover authenticated encryption, record substitution, plaintext migration,
locked storage, secure request completion, retry idempotency, cancellation and
failed verification. The ignored platform_keyring_roundtrip test is for an actual
unlocked OS credential store. Run it explicitly in a disposable Secret Service
session on Linux or the macOS login keychain.

For isolated real-process installation, credential recovery, OAuth protocol,
remote tool calls and live provider checks, see
[Connector live verification](../guides/connector-verification.md).
