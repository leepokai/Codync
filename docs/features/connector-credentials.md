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

Encrypted records include connector environment, headers, command arguments and
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

## Conversation requests

The built-in `connectors` MCP offers search_connectors, list_connectors,
request_connection, request_app (Composio), and request_secret. Requests become
notice entries with structured connectionRequest data. They identify the requesting
bot, service, destination field, purpose and status; they contain no credentials.

Clients submit values through connectorRequestFinish. The host validates the
request, destination, requesting bot and 24-hour expiry, stores credentials,
verifies availability, enables the connector for that bot, and queues a non-secret
acknowledgement in the original conversation/thread. Repeated completed submissions
return the completed entry. Cancelled requests reject submissions. Verification
failures remain pending so the user can correct credentials and retry.

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
