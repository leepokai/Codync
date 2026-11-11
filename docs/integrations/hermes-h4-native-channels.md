# Native Hermes messaging transports

Signal, Email and Home Assistant are installed through Extensions → Apps. The desktop,
iPhone and Android clients reuse their existing host-supplied app fields, encrypted
connector installation and bot connector selection. The terminal reuses the connector
catalog/install form through `codync-native/{signal,email,homeassistant}`. No client
contains platform credentials, authentication policy or routing logic.

## Source and adaptations

Hermes core remains pinned to `10d25f8ad65fdf71304e9272959421a0c22e5d23`.
`plugins/platforms/signal/adapter.py` and `plugins/platforms/email/adapter.py` are imported
from that installation rather than copied or reimplemented. `gateway/platforms/base.py`,
`gateway/config.py`, the scoped secret reader and the native environment loader retain
their names and permissions. The transport never imports the CLI/ACP entry point, which
can repair/update a checkout. It reuses the host's `acp_owner.py` profile/logging helpers
as `native_profile.py`; all published file contents participate in the runtime cache hash.

Home Assistant moved upstream to `NousResearch/hermes-official-plugins`. The exact MIT
files `homeassistant/adapter.py`, `homeassistant/tools.py` and `homeassistant/LICENSE`
from `e7a218f26db88426ab3e7376f0e000b6e356f5e7` are copied into
`host/src/native_channels/homeassistant/`. Only two provenance comment lines are added
to the Python sources. The local package initializer omits plugin/tool registration;
the bridge registers the native platform identity only. The tools module is retained
because the adapter imports its native URL validator. No new smart-home control tool
is silently enabled by installing an event channel.

`bridge.py` adapts the original adapter callbacks to bounded JSON lines. It retains native
Signal group/mention policy and Email sender authentication, checks host authorization
before downloading/extracting attachments, and never starts a second gateway agent.
The host owns a process tree per connection, durable inbox/outbox receipts, bot binding,
pairing, approvals, transcription and conversation threads.

## Existing connections and security

Blank optional fields reuse the selected Hermes profile's environment and native platform
configuration. Explicit fields retain native `SIGNAL_*`, `EMAIL_*` and `HASS_*` names and
are encrypted in the existing host credential vault. Each connection owns a separate
process/session. The native account identity is hashed into its generation; account
replacement, configuration replacement, connector removal and sender revocation fence
queued input and revoke matching active turns. A bot must have the connection enabled,
including when its ID is explicitly selected in the app fields.

An optional one-use DM code is seeded for ten minutes after connection. `/pair <code>`
does not download attachments or bypass authenticated Email From policy. Consumed/expired
codes remain tombstoned across reconnects. The local API also provides
`nativeChannelPairing`, `nativeChannelSetAllowed` and `nativeChannelStatus`.
Approval replies use `/approve <code> <number>` and require the exact original sender,
conversation, native generation, bot and still-pending ACP option. Mixed-author approval
cards remain in Codync rather than delegating authority to the last channel participant.

Files are bounded to ten attachments and 20 MiB total. Audio uses the existing host speech
connection and transcription memo. An attachment failure produces a clarification notice
without blocking the incoming FIFO. Outgoing native acknowledgement is required before
the durable receipt becomes sent. A possibly delivered send is marked unknown and is
never automatically replayed.

## Signed Email webhook

`POST /hooks/native-email/<connection-id>` accepts an RFC822 body up to 20 MiB when the
connection has a 32+ character `CODYNC_CHANNEL_WEBHOOK_SECRET`. Include exactly one
`x-codync-timestamp` header (Unix seconds, within five minutes) and exactly one
`x-codync-email-signature` header: `v1=<hex HMAC-SHA256(secret, timestamp + "." + raw-body)>`.
Native Email authentication still applies. The bounded durable spool is deduplicated;
completed records keep only their receipt, not the raw mail. A webhook provider must
forward trusted receiving-MTA authentication results matching `EMAIL_AUTHSERV_ID`.

## Validation boundary

Regression coverage is included for account-fenced inbox claims, one-use pairing,
revocation overrides, crash-ambiguous delivery and signed webhook freshness/body binding.
Per the user's current instruction, no compilation, tests, dependency installation,
providers, external sends or app execution were performed. Static Rust parsing/formatting
and Python AST parsing are the validation performed; live platform behavior remains
unverified. Original Signal/Email/HA service sessions and optional transport dependencies
must already be available in the pinned Hermes environment.
